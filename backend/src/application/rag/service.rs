//! Orchestrates the two pipelines on top of a [`RagEngine`] + the document/
//! chunk repositories. Both run synchronously within the request — same
//! choice the sibling Filers project made for its LLM-backed endpoints
//! (`/api/ocr`, `/api/pdf/extract`): a background-job/poll dance is more
//! moving parts than a PDF-sized ingest actually needs, and the `Document`
//! row plus a `Job` trace give the same observability either way.

use std::sync::Arc;

use chrono::Utc;
use uuid::Uuid;

use crate::{
    domain::rag::{
        entities::{Chunk, Document, DocumentStatus},
        repository::{ChunkRepository, DocumentRepository},
    },
    errors::{AppError, AppResult},
};

use super::{RagEngine, answer, chunker::chunk_text, embeddings::InputType, pii};

async fn fail(
    documents: &Arc<dyn DocumentRepository>,
    mut doc: Document,
    err: AppError,
) -> AppError {
    doc.status = DocumentStatus::Failed;
    doc.error = Some(err.to_string());
    doc.updated_at = Utc::now();
    let _ = documents.update(doc).await;
    err
}

/// Extract → chunk → embed → store, updating `doc`'s status as it goes so a
/// caller re-reading the row mid-flight (or after a crash) sees where it got
/// to. On failure the document is left in `Failed` with `error` set, never
/// silently half-ingested.
#[allow(clippy::too_many_arguments)]
pub async fn ingest_pdf(
    engine: &RagEngine,
    documents: &Arc<dyn DocumentRepository>,
    chunks_repo: &Arc<dyn ChunkRepository>,
    mut doc: Document,
    pdf_bytes: &[u8],
    redact_pii: bool,
) -> AppResult<(Document, u32, usize)> {
    doc.status = DocumentStatus::Chunking;
    doc.updated_at = Utc::now();
    documents.update(doc.clone()).await?;

    let text_result = match crate::application::processing::pdf::text::run(pdf_bytes, None) {
        Ok(t) => t,
        Err(e) => return Err(fail(documents, doc, e).await),
    };
    let pages = text_result.pages.len() as u32;

    let full_text = text_result
        .pages
        .iter()
        .map(|p| p.text.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    if full_text.trim().is_empty() {
        let err = AppError::BadRequest(
            "this PDF has no extractable text — likely a scanned document; \
             page-image OCR isn't supported yet"
                .into(),
        );
        return Err(fail(documents, doc, err).await);
    }

    ingest_text(engine, documents, chunks_repo, doc, &full_text, pages, redact_pii).await
}

/// Same pipeline as [`ingest_pdf`] from the point text is already in hand —
/// chunk → embed → store. Used directly by anything that isn't a PDF (e.g.
/// [`super::erp_sync`]'s ERP/CRM records), so a non-file knowledge source
/// gets identical chunking/embedding/status-tracking behavior for free
/// instead of a parallel, drifting implementation.
///
/// `redact_pii`: opt-in, best-effort [`pii::redact`] pass over the *whole*
/// text before it's chunked — so recognised PII (emails, IBANs, IDs, cards,
/// phone numbers) never reaches the embeddings/chat model and never lands in
/// Qdrant or `chunks.text`. Off by default: redaction is lossy, and a caller
/// who needs to ask "what's this customer's phone number?" later needs that
/// value still there.
#[allow(clippy::too_many_arguments)]
pub async fn ingest_text(
    engine: &RagEngine,
    documents: &Arc<dyn DocumentRepository>,
    chunks_repo: &Arc<dyn ChunkRepository>,
    mut doc: Document,
    full_text: &str,
    pages: u32,
    redact_pii: bool,
) -> AppResult<(Document, u32, usize)> {
    doc.pages = pages;
    doc.status = DocumentStatus::Chunking;
    doc.updated_at = Utc::now();
    documents.update(doc.clone()).await?;

    let (redacted_text, redacted_count) = if redact_pii {
        pii::redact(full_text)
    } else {
        (full_text.to_string(), 0)
    };
    let full_text = redacted_text.as_str();

    let spans = chunk_text(full_text, engine.chunk_size, engine.chunk_overlap);
    if spans.is_empty() {
        let err = AppError::Internal(anyhow::anyhow!(
            "chunker produced no spans for non-empty text"
        ));
        return Err(fail(documents, doc, err).await);
    }

    let chunk_entities: Vec<Chunk> = spans
        .iter()
        .enumerate()
        .map(|(i, s)| Chunk::new(doc.id, i as u32, s.text.clone(), s.char_start, s.char_end))
        .collect();

    doc.status = DocumentStatus::Embedding;
    doc.updated_at = Utc::now();
    documents.update(doc.clone()).await?;

    let texts: Vec<String> = chunk_entities.iter().map(|c| c.text.clone()).collect();
    let (vectors, tokens_used) = match engine.embeddings.embed(&texts, InputType::Passage).await {
        Ok(v) => v,
        Err(e) => return Err(fail(documents, doc, e).await),
    };

    if let Err(e) = engine
        .vector_store
        .upsert_chunks(doc.owner.as_deref(), doc.id, &chunk_entities, vectors)
        .await
    {
        return Err(fail(documents, doc, e).await);
    }

    if let Err(e) = chunks_repo.insert_many(chunk_entities.clone()).await {
        return Err(fail(documents, doc, e).await);
    }

    doc.status = DocumentStatus::Ready;
    doc.chunk_count = chunk_entities.len() as u32;
    doc.error = None;
    doc.updated_at = Utc::now();
    documents.update(doc.clone()).await?;
    Ok((doc, tokens_used.unwrap_or(0), redacted_count))
}

pub struct SourceOut {
    pub document_id: Uuid,
    pub chunk_index: u32,
    pub text: String,
    pub score: f32,
}

pub struct AnswerOutcome {
    pub answer: String,
    pub sources: Vec<SourceOut>,
    pub usage: Option<answer::Usage>,
    /// Tokens spent embedding the question itself — separate from `usage`
    /// (the answer-generation call), summed with it for AI-quota accounting.
    pub embedding_tokens: u32,
}

/// Embed `question`, retrieve the top matches (scoped to `owner`, optionally
/// narrowed to `document_ids`), and ask the chat model to answer grounded in
/// them. An empty retrieval isn't an error — it's a legitimate "nothing
/// relevant is indexed yet" answer.
pub async fn answer_question(
    engine: &RagEngine,
    owner: Option<&str>,
    document_ids: Option<&[Uuid]>,
    question: &str,
    top_k: Option<u64>,
) -> AppResult<AnswerOutcome> {
    let (q_vecs, embedding_tokens) = engine
        .embeddings
        .embed(&[question.to_string()], InputType::Query)
        .await?;
    let q_vec = q_vecs.into_iter().next().ok_or_else(|| {
        AppError::Internal(anyhow::anyhow!("no embedding returned for the question"))
    })?;
    let embedding_tokens = embedding_tokens.unwrap_or(0);

    let hits = engine
        .vector_store
        .search(owner, document_ids, q_vec, top_k.unwrap_or(engine.top_k))
        .await?;

    if hits.is_empty() {
        return Ok(AnswerOutcome {
            answer: "No relevant documents are indexed for this question yet.".into(),
            sources: Vec::new(),
            usage: None,
            embedding_tokens,
        });
    }

    let labels: Vec<String> = hits
        .iter()
        .map(|h| format!("document {} · chunk {}", h.document_id, h.chunk_index))
        .collect();
    let sources: Vec<answer::Source> = hits
        .iter()
        .zip(&labels)
        .map(|(h, label)| answer::Source {
            label,
            text: &h.text,
        })
        .collect();

    let (text, usage) = engine.answerer.answer(question, &sources).await?;

    Ok(AnswerOutcome {
        answer: text,
        sources: hits
            .into_iter()
            .map(|h| SourceOut {
                document_id: h.document_id,
                chunk_index: h.chunk_index,
                text: h.text,
                score: h.score,
            })
            .collect(),
        usage,
        embedding_tokens,
    })
}
