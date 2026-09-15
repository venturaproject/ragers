//! Documents ingested into the RAG index, and the chunks they were split
//! into. The embedding vector for each chunk lives only in Qdrant — these
//! tables hold metadata (and the chunk text, so the admin panel and
//! re-embedding-on-model-change don't need a round-trip to the vector store).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DocumentStatus {
    Pending,
    Chunking,
    Embedding,
    Ready,
    Failed,
}

impl DocumentStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Chunking => "chunking",
            Self::Embedding => "embedding",
            Self::Ready => "ready",
            Self::Failed => "failed",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(Self::Pending),
            "chunking" => Some(Self::Chunking),
            "embedding" => Some(Self::Embedding),
            "ready" => Some(Self::Ready),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub id: Uuid,
    /// Stable id of the principal that ingested it (user id / client id).
    /// `None` for a service key. Scopes `GET`/`DELETE` the same way
    /// `Job.owner` already does.
    #[serde(skip)]
    pub owner: Option<String>,
    pub filename: String,
    pub status: DocumentStatus,
    pub pages: u32,
    pub chunk_count: u32,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Document {
    pub fn new(owner: Option<String>, filename: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            owner,
            filename,
            status: DocumentStatus::Pending,
            pages: 0,
            chunk_count: 0,
            error: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "filename": self.filename,
            "status": self.status.as_str(),
            "pages": self.pages,
            "chunk_count": self.chunk_count,
            "error": self.error,
            "created_at": self.created_at.to_rfc3339(),
            "updated_at": self.updated_at.to_rfc3339(),
        })
    }
}

/// One chunk of a document's text, as sent to the embedding model and stored
/// (with its vector) in Qdrant. `index` is the chunk's position within the
/// document, `char_start`/`char_end` its offset in the extracted text — both
/// exist for citations ("this answer came from chunk 3 of invoice.pdf").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    pub id: Uuid,
    pub document_id: Uuid,
    pub index: u32,
    pub text: String,
    pub char_start: usize,
    pub char_end: usize,
}

impl Chunk {
    pub fn new(
        document_id: Uuid,
        index: u32,
        text: String,
        char_start: usize,
        char_end: usize,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            document_id,
            index,
            text,
            char_start,
            char_end,
        }
    }
}
