//! The RAG pipeline: `pdf::text` → [`chunker`] → [`embeddings`] →
//! [`vector_store`] on ingest; [`embeddings`] → [`vector_store`] →
//! [`answer`] on query.

pub mod answer;
pub mod chunker;
pub mod embeddings;
pub mod service;
pub mod vector_store;

use crate::config::Config;

use answer::AnswerClient;
use embeddings::EmbeddingsClient;
use vector_store::VectorStore;

/// Dimensionality of `EMBEDDINGS_MODEL`'s output vectors. NIM's
/// `nemotron-3-embed-1b` (the default) emits 2048-dim vectors; change this
/// alongside `EMBEDDINGS_MODEL` if you point at a different model — Qdrant's
/// collection is created with this size on first boot and can't be resized
/// in place (drop the collection, or change `QDRANT_COLLECTION`, if you
/// switch models on an existing deployment). NIM's embedding catalog turns
/// over fast — if this model is retired too, `GET /v1/models` against your
/// `LLM_BASE_URL` lists what's currently live for your account.
pub const EMBEDDING_DIM: u64 = 2048;

/// Embeddings + chat LLM + Qdrant, bundled behind one gate: `/api/rag/*`
/// needs all three, so [`AppState::rag`](crate::state::AppState::rag) is
/// `None` unless every one of them is configured and Qdrant was reachable at
/// startup — the routes then answer 503 instead of half-working.
pub struct RagEngine {
    pub embeddings: EmbeddingsClient,
    pub answerer: AnswerClient,
    pub vector_store: VectorStore,
    pub chunk_size: usize,
    pub chunk_overlap: usize,
    pub top_k: u64,
}

impl RagEngine {
    /// `None` when `LLM_API_KEY` is unset. `Err` when the key is set but
    /// Qdrant couldn't be reached/prepared — a startup-time failure, not a
    /// per-request one, since Qdrant is core to the service rather than an
    /// optional integration the way OCR was in the sibling project.
    pub async fn from_config(config: &Config) -> anyhow::Result<Option<Self>> {
        let Some(api_key) = config.llm_api_key.clone().filter(|k| !k.is_empty()) else {
            return Ok(None);
        };

        let embeddings = EmbeddingsClient::new(
            &config.llm_base_url,
            &api_key,
            &config.embeddings_model,
            config.llm_timeout_secs,
        )?;
        let answerer = AnswerClient::new(
            &config.llm_base_url,
            &api_key,
            &config.chat_model,
            config.llm_max_tokens,
            config.llm_timeout_secs,
        )?;
        let vector_store =
            VectorStore::connect(&config.qdrant_url, &config.qdrant_collection, EMBEDDING_DIM)
                .await?;

        Ok(Some(Self {
            embeddings,
            answerer,
            vector_store,
            chunk_size: config.chunk_size_chars,
            chunk_overlap: config.chunk_overlap_chars,
            top_k: config.rag_top_k,
        }))
    }
}
