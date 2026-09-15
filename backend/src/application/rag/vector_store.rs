//! Thin wrapper around the Qdrant client — one collection
//! (`QDRANT_COLLECTION`), multi-tenant via an `owner` payload field rather
//! than one collection per tenant (Qdrant's documented approach for
//! small/medium multi-tenant RAG — provisioning a collection per API client
//! doesn't scale operationally and isn't needed at this size).

use qdrant_client::qdrant::{
    Condition, CreateCollectionBuilder, DeletePointsBuilder, Distance, Filter, PointStruct,
    QueryPointsBuilder, UpsertPointsBuilder, VectorParamsBuilder,
};
use qdrant_client::{Payload, Qdrant};
use uuid::Uuid;

use crate::domain::rag::entities::Chunk;
use crate::errors::{AppError, AppResult};

fn map_err(e: impl std::fmt::Display) -> AppError {
    AppError::Internal(anyhow::anyhow!("Qdrant error: {e}"))
}

pub struct SearchHit {
    pub chunk_id: Uuid,
    pub document_id: Uuid,
    pub chunk_index: u32,
    pub text: String,
    pub score: f32,
}

pub struct VectorStore {
    client: Qdrant,
    collection: String,
}

impl VectorStore {
    /// Connect and make sure the collection exists (creating it with
    /// `vector_size` dimensions if this is the first run). Idempotent —
    /// safe to call on every startup.
    pub async fn connect(url: &str, collection: &str, vector_size: u64) -> AppResult<Self> {
        let client = Qdrant::from_url(url).build().map_err(map_err)?;
        let store = Self {
            client,
            collection: collection.to_string(),
        };
        store.ensure_collection(vector_size).await?;
        Ok(store)
    }

    async fn ensure_collection(&self, vector_size: u64) -> AppResult<()> {
        let exists = self
            .client
            .collection_exists(&self.collection)
            .await
            .map_err(map_err)?;
        if !exists {
            self.client
                .create_collection(
                    CreateCollectionBuilder::new(&self.collection)
                        .vectors_config(VectorParamsBuilder::new(vector_size, Distance::Cosine)),
                )
                .await
                .map_err(map_err)?;
            tracing::info!(collection = %self.collection, %vector_size, "created Qdrant collection");
        }
        Ok(())
    }

    pub async fn health(&self) -> bool {
        self.client.health_check().await.is_ok()
    }

    /// One point per chunk, `vectors[i]` for `chunks[i]`. `owner` (may be
    /// `None` for a service-key caller — same convention as `Job.owner`) and
    /// `document_id` are stored in the payload for filtering at query time.
    pub async fn upsert_chunks(
        &self,
        owner: Option<&str>,
        document_id: Uuid,
        chunks: &[Chunk],
        vectors: Vec<Vec<f32>>,
    ) -> AppResult<()> {
        let points: Vec<PointStruct> = chunks
            .iter()
            .zip(vectors)
            .map(|(chunk, vector)| {
                let payload: Payload = serde_json::json!({
                    "owner": owner,
                    "document_id": document_id.to_string(),
                    "chunk_index": chunk.index,
                    "text": chunk.text,
                })
                .try_into()
                .unwrap_or_default();
                PointStruct::new(chunk.id.to_string(), vector, payload)
            })
            .collect();

        self.client
            .upsert_points(UpsertPointsBuilder::new(&self.collection, points))
            .await
            .map_err(map_err)?;
        Ok(())
    }

    /// Top-`top_k` chunks by cosine similarity to `query_vector`, scoped to
    /// `owner` and optionally narrowed to `document_ids`.
    pub async fn search(
        &self,
        owner: Option<&str>,
        document_ids: Option<&[Uuid]>,
        query_vector: Vec<f32>,
        top_k: u64,
    ) -> AppResult<Vec<SearchHit>> {
        let mut must: Vec<Condition> = Vec::new();
        if let Some(o) = owner {
            must.push(Condition::matches("owner", o.to_string()));
        }
        if let Some(ids) = document_ids.filter(|ids| !ids.is_empty()) {
            let values: Vec<String> = ids.iter().map(Uuid::to_string).collect();
            must.push(Condition::matches("document_id", values));
        }

        let mut query = QueryPointsBuilder::new(&self.collection)
            .query(query_vector)
            .limit(top_k)
            .with_payload(true);
        if !must.is_empty() {
            query = query.filter(Filter::must(must));
        }

        let resp = self.client.query(query).await.map_err(map_err)?;

        resp.result
            .into_iter()
            .map(|point| {
                let chunk_id: Uuid = point
                    .id
                    .as_ref()
                    .and_then(|id| id.point_id_options.as_ref())
                    .map(|opt| match opt {
                        qdrant_client::qdrant::point_id::PointIdOptions::Uuid(s) => s.clone(),
                        qdrant_client::qdrant::point_id::PointIdOptions::Num(n) => n.to_string(),
                    })
                    .and_then(|s| Uuid::parse_str(&s).ok())
                    .ok_or_else(|| {
                        AppError::Internal(anyhow::anyhow!("Qdrant point had no readable id"))
                    })?;
                let document_id: Uuid = point
                    .payload
                    .get("document_id")
                    .and_then(|v| v.as_str())
                    .and_then(|s| Uuid::parse_str(s).ok())
                    .ok_or_else(|| {
                        AppError::Internal(anyhow::anyhow!(
                            "Qdrant point payload missing document_id"
                        ))
                    })?;
                let chunk_index = point
                    .payload
                    .get("chunk_index")
                    .and_then(|v| v.as_integer())
                    .unwrap_or(0) as u32;
                let text = point
                    .payload
                    .get("text")
                    .and_then(|v| v.as_str())
                    .cloned()
                    .unwrap_or_default();
                Ok(SearchHit {
                    chunk_id,
                    document_id,
                    chunk_index,
                    text,
                    score: point.score,
                })
            })
            .collect()
    }

    /// Purge every point belonging to `document_id` — called when a document
    /// is deleted, so the vector store never outlives its Postgres metadata.
    pub async fn delete_document(&self, document_id: Uuid) -> AppResult<()> {
        self.client
            .delete_points(
                DeletePointsBuilder::new(&self.collection).points(Filter::must([
                    Condition::matches("document_id", document_id.to_string()),
                ])),
            )
            .await
            .map_err(map_err)?;
        Ok(())
    }
}
