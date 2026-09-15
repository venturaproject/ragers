use async_trait::async_trait;
use chrono::Utc;
use dashmap::DashMap;
use uuid::Uuid;

use crate::{
    domain::chat::{
        entities::{ChatMessage, ChatThread},
        repository::{ChatMessageRepository, ChatThreadRepository},
    },
    errors::{AppError, AppResult},
};

pub struct MemoryChatThreadRepository {
    store: DashMap<Uuid, ChatThread>,
}

impl MemoryChatThreadRepository {
    pub fn new() -> Self {
        Self {
            store: DashMap::new(),
        }
    }
}

impl Default for MemoryChatThreadRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ChatThreadRepository for MemoryChatThreadRepository {
    async fn create(&self, thread: ChatThread) -> AppResult<()> {
        self.store.insert(thread.id, thread);
        Ok(())
    }

    async fn find(&self, id: Uuid) -> AppResult<Option<ChatThread>> {
        Ok(self.store.get(&id).map(|r| r.clone()))
    }

    async fn list(&self, owner: Option<&str>) -> AppResult<Vec<ChatThread>> {
        let mut threads: Vec<ChatThread> = self
            .store
            .iter()
            .filter(|e| owner.is_none() || e.owner.as_deref() == owner)
            .map(|r| r.clone())
            .collect();
        threads.sort_by_key(|t| std::cmp::Reverse(t.updated_at));
        Ok(threads)
    }

    async fn touch(&self, id: Uuid) -> AppResult<()> {
        let mut thread = self
            .store
            .get_mut(&id)
            .ok_or_else(|| AppError::NotFound(format!("Chat thread {id}")))?;
        thread.updated_at = Utc::now();
        Ok(())
    }

    async fn rename(&self, id: Uuid, title: String) -> AppResult<()> {
        let mut thread = self
            .store
            .get_mut(&id)
            .ok_or_else(|| AppError::NotFound(format!("Chat thread {id}")))?;
        thread.title = title;
        thread.updated_at = Utc::now();
        Ok(())
    }

    async fn delete(&self, id: Uuid) -> AppResult<()> {
        self.store.remove(&id);
        Ok(())
    }
}

pub struct MemoryChatMessageRepository {
    store: DashMap<Uuid, Vec<ChatMessage>>,
}

impl MemoryChatMessageRepository {
    pub fn new() -> Self {
        Self {
            store: DashMap::new(),
        }
    }
}

impl Default for MemoryChatMessageRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ChatMessageRepository for MemoryChatMessageRepository {
    async fn create(&self, message: ChatMessage) -> AppResult<()> {
        self.store.entry(message.thread_id).or_default().push(message);
        Ok(())
    }

    async fn list_for_thread(&self, thread_id: Uuid) -> AppResult<Vec<ChatMessage>> {
        Ok(self
            .store
            .get(&thread_id)
            .map(|r| r.clone())
            .unwrap_or_default())
    }

    async fn delete_for_thread(&self, thread_id: Uuid) -> AppResult<()> {
        self.store.remove(&thread_id);
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::domain::chat::entities::{ChatMessage, MessageRole};

    #[tokio::test]
    async fn threads_are_scoped_by_owner_unless_none() {
        let repo = MemoryChatThreadRepository::new();
        repo.create(ChatThread::new(Some("a".into()), "hilo a".into()))
            .await
            .unwrap();
        repo.create(ChatThread::new(Some("b".into()), "hilo b".into()))
            .await
            .unwrap();

        assert_eq!(repo.list(Some("a")).await.unwrap().len(), 1);
        assert_eq!(repo.list(None).await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn rename_updates_title_and_bumps_updated_at() {
        let repo = MemoryChatThreadRepository::new();
        let thread = ChatThread::new(Some("a".into()), "original".into());
        let id = thread.id;
        let original_updated_at = thread.updated_at;
        repo.create(thread).await.unwrap();

        repo.rename(id, "nuevo nombre".into()).await.unwrap();

        let renamed = repo.find(id).await.unwrap().unwrap();
        assert_eq!(renamed.title, "nuevo nombre");
        assert!(renamed.updated_at >= original_updated_at);
    }

    #[tokio::test]
    async fn rename_and_touch_fail_for_unknown_thread() {
        let repo = MemoryChatThreadRepository::new();
        let missing = Uuid::new_v4();
        assert!(repo.rename(missing, "x".into()).await.is_err());
        assert!(repo.touch(missing).await.is_err());
    }

    #[tokio::test]
    async fn delete_removes_the_thread() {
        let repo = MemoryChatThreadRepository::new();
        let thread = ChatThread::new(None, "temporal".into());
        let id = thread.id;
        repo.create(thread).await.unwrap();

        repo.delete(id).await.unwrap();

        assert!(repo.find(id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn messages_are_grouped_and_deletable_per_thread() {
        let repo = MemoryChatMessageRepository::new();
        let thread_id = Uuid::new_v4();
        repo.create(ChatMessage::new(
            thread_id,
            MessageRole::User,
            "hola".into(),
            None,
        ))
        .await
        .unwrap();
        repo.create(ChatMessage::new(
            thread_id,
            MessageRole::Assistant,
            "respuesta".into(),
            None,
        ))
        .await
        .unwrap();

        assert_eq!(repo.list_for_thread(thread_id).await.unwrap().len(), 2);
        repo.delete_for_thread(thread_id).await.unwrap();
        assert!(repo.list_for_thread(thread_id).await.unwrap().is_empty());
    }
}
