use async_trait::async_trait;
use chrono::Utc;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::domain::chat::{
    entities::{ChatMessage, ChatThread, MessageRole},
    repository::{ChatMessageRepository, ChatThreadRepository},
};
use crate::errors::{AppError, AppResult};

fn map_err(e: sqlx::Error) -> AppError {
    AppError::Internal(anyhow::anyhow!(e))
}

// ── Threads ──────────────────────────────────────────────────────────────────

pub struct PgChatThreadRepository {
    pool: PgPool,
}

impl PgChatThreadRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn row_to_thread(row: &sqlx::postgres::PgRow) -> AppResult<ChatThread> {
    Ok(ChatThread {
        id: row.try_get("id").map_err(map_err)?,
        owner: row.try_get("owner").map_err(map_err)?,
        title: row.try_get("title").map_err(map_err)?,
        created_at: row.try_get("created_at").map_err(map_err)?,
        updated_at: row.try_get("updated_at").map_err(map_err)?,
    })
}

#[async_trait]
impl ChatThreadRepository for PgChatThreadRepository {
    async fn create(&self, thread: ChatThread) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO chat_threads (id, owner, title, created_at, updated_at) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(thread.id)
        .bind(&thread.owner)
        .bind(&thread.title)
        .bind(thread.created_at)
        .bind(thread.updated_at)
        .execute(&self.pool)
        .await
        .map_err(map_err)?;
        Ok(())
    }

    async fn find(&self, id: Uuid) -> AppResult<Option<ChatThread>> {
        let row = sqlx::query(
            "SELECT id, owner, title, created_at, updated_at FROM chat_threads WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_err)?;
        row.as_ref().map(row_to_thread).transpose()
    }

    async fn list(&self, owner: Option<&str>) -> AppResult<Vec<ChatThread>> {
        let rows = match owner {
            Some(o) => sqlx::query(
                "SELECT id, owner, title, created_at, updated_at FROM chat_threads \
                 WHERE owner = $1 ORDER BY updated_at DESC LIMIT 500",
            )
            .bind(o)
            .fetch_all(&self.pool)
            .await,
            None => sqlx::query(
                "SELECT id, owner, title, created_at, updated_at FROM chat_threads \
                 ORDER BY updated_at DESC LIMIT 500",
            )
            .fetch_all(&self.pool)
            .await,
        }
        .map_err(map_err)?;

        Ok(rows
            .iter()
            .filter_map(|row| match row_to_thread(row) {
                Ok(t) => Some(t),
                Err(e) => {
                    tracing::warn!("skipping unreadable chat thread row: {e}");
                    None
                }
            })
            .collect())
    }

    async fn touch(&self, id: Uuid) -> AppResult<()> {
        let res = sqlx::query("UPDATE chat_threads SET updated_at = $2 WHERE id = $1")
            .bind(id)
            .bind(Utc::now())
            .execute(&self.pool)
            .await
            .map_err(map_err)?;
        if res.rows_affected() == 0 {
            return Err(AppError::NotFound(format!("Chat thread {id}")));
        }
        Ok(())
    }

    async fn rename(&self, id: Uuid, title: String) -> AppResult<()> {
        let res = sqlx::query("UPDATE chat_threads SET title = $2, updated_at = $3 WHERE id = $1")
            .bind(id)
            .bind(title)
            .bind(Utc::now())
            .execute(&self.pool)
            .await
            .map_err(map_err)?;
        if res.rows_affected() == 0 {
            return Err(AppError::NotFound(format!("Chat thread {id}")));
        }
        Ok(())
    }

    async fn delete(&self, id: Uuid) -> AppResult<()> {
        sqlx::query("DELETE FROM chat_threads WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(map_err)?;
        Ok(())
    }
}

// ── Messages ─────────────────────────────────────────────────────────────────

pub struct PgChatMessageRepository {
    pool: PgPool,
}

impl PgChatMessageRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ChatMessageRepository for PgChatMessageRepository {
    async fn create(&self, message: ChatMessage) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO chat_messages (id, thread_id, role, content, sources, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(message.id)
        .bind(message.thread_id)
        .bind(message.role.as_str())
        .bind(&message.content)
        .bind(&message.sources)
        .bind(message.created_at)
        .execute(&self.pool)
        .await
        .map_err(map_err)?;
        Ok(())
    }

    async fn list_for_thread(&self, thread_id: Uuid) -> AppResult<Vec<ChatMessage>> {
        let rows = sqlx::query(
            "SELECT id, thread_id, role, content, sources, created_at FROM chat_messages \
             WHERE thread_id = $1 ORDER BY created_at ASC",
        )
        .bind(thread_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;

        rows.iter()
            .map(|row| {
                let role_raw: String = row.try_get("role").map_err(map_err)?;
                let role = MessageRole::parse(&role_raw).ok_or_else(|| {
                    AppError::Internal(anyhow::anyhow!("unknown chat message role: {role_raw}"))
                })?;
                Ok(ChatMessage {
                    id: row.try_get("id").map_err(map_err)?,
                    thread_id: row.try_get("thread_id").map_err(map_err)?,
                    role,
                    content: row.try_get("content").map_err(map_err)?,
                    sources: row.try_get("sources").map_err(map_err)?,
                    created_at: row.try_get("created_at").map_err(map_err)?,
                })
            })
            .collect()
    }

    async fn delete_for_thread(&self, thread_id: Uuid) -> AppResult<()> {
        sqlx::query("DELETE FROM chat_messages WHERE thread_id = $1")
            .bind(thread_id)
            .execute(&self.pool)
            .await
            .map_err(map_err)?;
        Ok(())
    }
}
