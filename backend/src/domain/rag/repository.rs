use async_trait::async_trait;
use uuid::Uuid;

use super::entities::{Chunk, Document};
use crate::errors::AppResult;

#[async_trait]
pub trait DocumentRepository: Send + Sync {
    async fn create(&self, doc: Document) -> AppResult<()>;
    async fn find(&self, id: Uuid) -> AppResult<Option<Document>>;
    async fn update(&self, doc: Document) -> AppResult<()>;
    /// `owner = None` → every document (privileged callers); `Some(key)` →
    /// only that owner's, newest first.
    async fn list(&self, owner: Option<&str>) -> AppResult<Vec<Document>>;
    async fn delete(&self, id: Uuid) -> AppResult<()>;
}

#[async_trait]
pub trait ChunkRepository: Send + Sync {
    async fn insert_many(&self, chunks: Vec<Chunk>) -> AppResult<()>;
    async fn list_for_document(&self, document_id: Uuid) -> AppResult<Vec<Chunk>>;
    async fn delete_for_document(&self, document_id: Uuid) -> AppResult<()>;
}
