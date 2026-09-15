//! Persisted chat threads for the admin "Chat RAG" page — a record of what
//! was asked and answered via `POST /api/rag/query`, not conversation memory
//! for the model. See `application::chat::service`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatThread {
    pub id: Uuid,
    /// Stable id of the principal that owns it (user id / client id). `None`
    /// for a service key. Scopes list/delete the same way `Document.owner` does.
    #[serde(skip)]
    pub owner: Option<String>,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ChatThread {
    pub fn new(owner: Option<String>, title: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            owner,
            title,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "title": self.title,
            "created_at": self.created_at.to_rfc3339(),
            "updated_at": self.updated_at.to_rfc3339(),
        })
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    User,
    Assistant,
}

impl MessageRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "user" => Some(Self::User),
            "assistant" => Some(Self::Assistant),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: Uuid,
    pub thread_id: Uuid,
    pub role: MessageRole,
    pub content: String,
    /// The RAG sources behind an assistant message (`None` for a user
    /// message), stored as-is for display — same shape as `QuerySource`.
    pub sources: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

impl ChatMessage {
    pub fn new(
        thread_id: Uuid,
        role: MessageRole,
        content: String,
        sources: Option<serde_json::Value>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            thread_id,
            role,
            content,
            sources,
            created_at: Utc::now(),
        }
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "role": self.role.as_str(),
            "content": self.content,
            "sources": self.sources,
            "created_at": self.created_at.to_rfc3339(),
        })
    }
}
