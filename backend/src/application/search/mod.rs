//! Embedded BM25 full-text index over ingested chunks — `tantivy`, in-process,
//! no external service. Runs alongside the Qdrant vector index as a second,
//! independent way to find the same content: exact keyword/phrase matches
//! (a SKU, an order number, an exact name) that semantic embeddings are
//! deliberately fuzzy about. Unlike RAG, this needs no `LLM_API_KEY` — it's
//! plain keyword search, free and local, a caller can use even when the LLM
//! integration is turned off.
//!
//! In-memory (`Index::create_in_ram`): rebuilt from whatever's in
//! `documents`/`chunks` at startup (see `bootstrap::warm_search_index`) —
//! simpler than managing an on-disk index file/volume, and cheap to rebuild
//! at boilerplate scale. A deployment indexing enough text for a full
//! rebuild to matter would want a `MmapDirectory` on a persistent volume
//! instead.

use std::sync::Arc;

use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, Occur, Query, QueryParser, TermQuery};
use tantivy::schema::{IndexRecordOption, STORED, STRING, Schema, TEXT, Value};
use tantivy::{Index, IndexReader, IndexWriter, TantivyDocument, Term};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::domain::rag::entities::Chunk;
use crate::errors::{AppError, AppResult};

fn map_err(e: impl std::fmt::Display) -> AppError {
    AppError::Internal(anyhow::anyhow!("search index error: {e}"))
}

pub struct SearchHit {
    pub document_id: Uuid,
    pub chunk_index: u32,
    pub text: String,
    pub score: f32,
}

pub struct SearchIndex {
    index: Index,
    // `commit` needs `&mut IndexWriter`; `add_document`/`delete_term` only
    // need `&self`. The lock is held only for the (blocking, in-memory,
    // fast) tantivy call itself, never across an `.await` of anything else.
    writer: Mutex<IndexWriter>,
    reader: IndexReader,
    f_document_id: tantivy::schema::Field,
    f_owner: tantivy::schema::Field,
    f_chunk_index: tantivy::schema::Field,
    f_text: tantivy::schema::Field,
}

impl SearchIndex {
    pub fn new() -> AppResult<Self> {
        let mut builder = Schema::builder();
        let f_document_id = builder.add_text_field("document_id", STRING | STORED);
        let f_owner = builder.add_text_field("owner", STRING | STORED);
        let f_chunk_index = builder.add_u64_field("chunk_index", STORED);
        let f_text = builder.add_text_field("text", TEXT | STORED);
        let schema = builder.build();

        let index = Index::create_in_ram(schema);
        let writer: IndexWriter = index.writer(50_000_000).map_err(map_err)?;
        let reader = index.reader().map_err(map_err)?;

        Ok(Self {
            index,
            writer: Mutex::new(writer),
            reader,
            f_document_id,
            f_owner,
            f_chunk_index,
            f_text,
        })
    }

    /// Queue a chunk for indexing — call [`Self::commit`] afterward (once
    /// per batch, not per chunk) to make it visible to searches.
    pub async fn index_chunk(&self, owner: Option<&str>, document_id: Uuid, chunk: &Chunk) -> AppResult<()> {
        let mut doc = TantivyDocument::default();
        doc.add_text(self.f_document_id, document_id.to_string());
        if let Some(o) = owner {
            doc.add_text(self.f_owner, o);
        }
        doc.add_u64(self.f_chunk_index, chunk.index as u64);
        doc.add_text(self.f_text, &chunk.text);

        let writer = self.writer.lock().await;
        writer.add_document(doc).map_err(map_err)?;
        Ok(())
    }

    /// Queue every chunk of `document_id` for removal — call
    /// [`Self::commit`] afterward.
    pub async fn delete_document(&self, document_id: Uuid) -> AppResult<()> {
        let writer = self.writer.lock().await;
        let term = Term::from_field_text(self.f_document_id, &document_id.to_string());
        writer.delete_term(term);
        Ok(())
    }

    pub async fn commit(&self) -> AppResult<()> {
        let mut writer = self.writer.lock().await;
        writer.commit().map_err(map_err)?;
        drop(writer);
        self.reader.reload().map_err(map_err)?;
        Ok(())
    }

    /// BM25 search, optionally scoped to `owner` — same multi-tenant
    /// convention as the vector store (`None` = every tenant, for
    /// privileged callers only; the HTTP handler decides which).
    pub fn search(&self, owner: Option<&str>, query_str: &str, limit: usize) -> AppResult<Vec<SearchHit>> {
        let searcher = self.reader.searcher();
        let parser = QueryParser::for_index(&self.index, vec![self.f_text]);
        let text_query = parser.parse_query(query_str).map_err(map_err)?;

        let query: Box<dyn Query> = match owner {
            Some(o) => {
                let owner_term = Term::from_field_text(self.f_owner, o);
                let owner_query: Box<dyn Query> =
                    Box::new(TermQuery::new(owner_term, IndexRecordOption::Basic));
                Box::new(BooleanQuery::new(vec![
                    (Occur::Must, text_query),
                    (Occur::Must, owner_query),
                ]))
            }
            None => text_query,
        };

        let top_docs = searcher
            .search(&query, &TopDocs::with_limit(limit).order_by_score())
            .map_err(map_err)?;

        let mut hits = Vec::with_capacity(top_docs.len());
        for (score, address) in top_docs {
            let doc: TantivyDocument = searcher.doc(address).map_err(map_err)?;
            let document_id = doc
                .get_first(self.f_document_id)
                .and_then(|v| v.as_str())
                .and_then(|s| Uuid::parse_str(s).ok())
                .ok_or_else(|| AppError::Internal(anyhow::anyhow!("search hit missing document_id")))?;
            let chunk_index = doc
                .get_first(self.f_chunk_index)
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;
            let text = doc
                .get_first(self.f_text)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            hits.push(SearchHit {
                document_id,
                chunk_index,
                text,
                score,
            });
        }
        Ok(hits)
    }
}

/// Read every chunk already in `documents`/`chunks` (all owners — this is a
/// startup warm-up, not a request) and index it, so a restart doesn't leave
/// the (in-memory) search index empty while Qdrant and Postgres still have
/// everything. Best-effort: a read failure is logged, not fatal to startup.
pub async fn warm_from_repositories(
    search: &SearchIndex,
    documents: &Arc<dyn crate::domain::rag::repository::DocumentRepository>,
    chunks: &Arc<dyn crate::domain::rag::repository::ChunkRepository>,
) {
    let docs = match documents.list(None).await {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!("search index warm-up: couldn't list documents: {e}");
            return;
        }
    };
    let mut indexed = 0usize;
    for doc in &docs {
        let doc_chunks = match chunks.list_for_document(doc.id).await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("search index warm-up: couldn't list chunks for {}: {e}", doc.id);
                continue;
            }
        };
        for chunk in &doc_chunks {
            if search
                .index_chunk(doc.owner.as_deref(), doc.id, chunk)
                .await
                .is_ok()
            {
                indexed += 1;
            }
        }
    }
    if let Err(e) = search.commit().await {
        tracing::warn!("search index warm-up: commit failed: {e}");
        return;
    }
    tracing::info!("search index: warmed up with {indexed} chunk(s) from {} document(s)", docs.len());
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn chunk(index: u32, text: &str) -> Chunk {
        Chunk::new(Uuid::new_v4(), index, text.to_string(), 0, text.len())
    }

    #[tokio::test]
    async fn finds_an_indexed_chunk_by_keyword() {
        let search = SearchIndex::new().unwrap();
        let doc_id = Uuid::new_v4();
        search
            .index_chunk(Some("owner-a"), doc_id, &chunk(0, "Mayor S.A. esta en Alcobendas"))
            .await
            .unwrap();
        search.commit().await.unwrap();

        let hits = search.search(None, "Alcobendas", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].document_id, doc_id);
    }

    #[tokio::test]
    async fn scopes_results_by_owner() {
        let search = SearchIndex::new().unwrap();
        search
            .index_chunk(Some("owner-a"), Uuid::new_v4(), &chunk(0, "factura numero uno"))
            .await
            .unwrap();
        search
            .index_chunk(Some("owner-b"), Uuid::new_v4(), &chunk(0, "factura numero dos"))
            .await
            .unwrap();
        search.commit().await.unwrap();

        assert_eq!(search.search(Some("owner-a"), "factura", 10).unwrap().len(), 1);
        assert_eq!(search.search(None, "factura", 10).unwrap().len(), 2);
    }

    #[tokio::test]
    async fn delete_document_removes_all_its_chunks() {
        let search = SearchIndex::new().unwrap();
        let doc_id = Uuid::new_v4();
        search.index_chunk(None, doc_id, &chunk(0, "primero")).await.unwrap();
        search.index_chunk(None, doc_id, &chunk(1, "segundo")).await.unwrap();
        search.commit().await.unwrap();
        assert_eq!(search.search(None, "primero OR segundo", 10).unwrap().len(), 2);

        search.delete_document(doc_id).await.unwrap();
        search.commit().await.unwrap();

        assert_eq!(search.search(None, "primero OR segundo", 10).unwrap().len(), 0);
    }
}
