use async_trait::async_trait;
use dashmap::DashMap;
use uuid::Uuid;

use crate::{
    domain::rag::{
        entities::{Chunk, Document},
        repository::{ChunkRepository, DocumentRepository},
    },
    errors::{AppError, AppResult},
};

pub struct MemoryDocumentRepository {
    store: DashMap<Uuid, Document>,
}

impl MemoryDocumentRepository {
    pub fn new() -> Self {
        Self {
            store: DashMap::new(),
        }
    }
}

impl Default for MemoryDocumentRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DocumentRepository for MemoryDocumentRepository {
    async fn create(&self, doc: Document) -> AppResult<()> {
        self.store.insert(doc.id, doc);
        Ok(())
    }

    async fn find(&self, id: Uuid) -> AppResult<Option<Document>> {
        Ok(self.store.get(&id).map(|r| r.clone()))
    }

    async fn update(&self, doc: Document) -> AppResult<()> {
        self.store
            .get_mut(&doc.id)
            .ok_or_else(|| AppError::NotFound(format!("Document {}", doc.id)))?
            .clone_from(&doc);
        Ok(())
    }

    async fn list(&self, owner: Option<&str>) -> AppResult<Vec<Document>> {
        let mut docs: Vec<Document> = self
            .store
            .iter()
            .filter(|e| owner.is_none() || e.owner.as_deref() == owner)
            .map(|r| r.clone())
            .collect();
        docs.sort_by_key(|d| std::cmp::Reverse(d.created_at));
        Ok(docs)
    }

    async fn delete(&self, id: Uuid) -> AppResult<()> {
        self.store.remove(&id);
        Ok(())
    }
}

pub struct MemoryChunkRepository {
    store: DashMap<Uuid, Vec<Chunk>>,
}

impl MemoryChunkRepository {
    pub fn new() -> Self {
        Self {
            store: DashMap::new(),
        }
    }
}

impl Default for MemoryChunkRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ChunkRepository for MemoryChunkRepository {
    async fn insert_many(&self, chunks: Vec<Chunk>) -> AppResult<()> {
        for chunk in chunks {
            self.store.entry(chunk.document_id).or_default().push(chunk);
        }
        Ok(())
    }

    async fn list_for_document(&self, document_id: Uuid) -> AppResult<Vec<Chunk>> {
        Ok(self
            .store
            .get(&document_id)
            .map(|r| r.clone())
            .unwrap_or_default())
    }

    async fn delete_for_document(&self, document_id: Uuid) -> AppResult<()> {
        self.store.remove(&document_id);
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn list_scopes_by_owner_unless_none() {
        let repo = MemoryDocumentRepository::new();
        repo.create(Document::new(Some("a".into()), "x.pdf".into()))
            .await
            .unwrap();
        repo.create(Document::new(Some("b".into()), "y.pdf".into()))
            .await
            .unwrap();

        assert_eq!(repo.list(Some("a")).await.unwrap().len(), 1);
        assert_eq!(repo.list(None).await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn chunks_are_grouped_and_deletable_per_document() {
        let repo = MemoryChunkRepository::new();
        let doc_id = Uuid::new_v4();
        repo.insert_many(vec![
            Chunk::new(doc_id, 0, "a".into(), 0, 1),
            Chunk::new(doc_id, 1, "b".into(), 1, 2),
        ])
        .await
        .unwrap();

        assert_eq!(repo.list_for_document(doc_id).await.unwrap().len(), 2);
        repo.delete_for_document(doc_id).await.unwrap();
        assert!(repo.list_for_document(doc_id).await.unwrap().is_empty());
    }
}
