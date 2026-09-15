use async_trait::async_trait;
use uuid::Uuid;

use super::entities::{ChatMessage, ChatThread};
use crate::errors::AppResult;

#[async_trait]
pub trait ChatThreadRepository: Send + Sync {
    async fn create(&self, thread: ChatThread) -> AppResult<()>;
    async fn find(&self, id: Uuid) -> AppResult<Option<ChatThread>>;
    /// `owner = None` → every thread (privileged callers); `Some(key)` →
    /// only that owner's, most recently updated first.
    async fn list(&self, owner: Option<&str>) -> AppResult<Vec<ChatThread>>;
    /// Bump `updated_at` so the thread list sorts by recent activity.
    async fn touch(&self, id: Uuid) -> AppResult<()>;
    async fn rename(&self, id: Uuid, title: String) -> AppResult<()>;
    async fn delete(&self, id: Uuid) -> AppResult<()>;
}

#[async_trait]
pub trait ChatMessageRepository: Send + Sync {
    async fn create(&self, message: ChatMessage) -> AppResult<()>;
    async fn list_for_thread(&self, thread_id: Uuid) -> AppResult<Vec<ChatMessage>>;
    async fn delete_for_thread(&self, thread_id: Uuid) -> AppResult<()>;
}
