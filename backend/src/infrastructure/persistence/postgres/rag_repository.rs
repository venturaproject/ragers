use async_trait::async_trait;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::domain::rag::{
    entities::{Chunk, Document, DocumentStatus},
    repository::{ChunkRepository, DocumentRepository},
};
use crate::errors::{AppError, AppResult};

fn map_err(e: sqlx::Error) -> AppError {
    AppError::Internal(anyhow::anyhow!(e))
}

// ── Documents ────────────────────────────────────────────────────────────────

pub struct PgDocumentRepository {
    pool: PgPool,
}

impl PgDocumentRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn row_to_document(row: &sqlx::postgres::PgRow) -> AppResult<Document> {
    let status_raw: String = row.try_get("status").map_err(map_err)?;
    let status = DocumentStatus::parse(&status_raw).ok_or_else(|| {
        AppError::Internal(anyhow::anyhow!("unknown document status: {status_raw}"))
    })?;
    Ok(Document {
        id: row.try_get("id").map_err(map_err)?,
        owner: row.try_get("owner").map_err(map_err)?,
        filename: row.try_get("filename").map_err(map_err)?,
        status,
        pages: row.try_get::<i32, _>("pages").map_err(map_err)?.max(0) as u32,
        chunk_count: row
            .try_get::<i32, _>("chunk_count")
            .map_err(map_err)?
            .max(0) as u32,
        error: row.try_get("error").map_err(map_err)?,
        created_at: row.try_get("created_at").map_err(map_err)?,
        updated_at: row.try_get("updated_at").map_err(map_err)?,
    })
}

#[async_trait]
impl DocumentRepository for PgDocumentRepository {
    async fn create(&self, doc: Document) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO documents \
                (id, owner, filename, status, pages, chunk_count, error, created_at, updated_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(doc.id)
        .bind(&doc.owner)
        .bind(&doc.filename)
        .bind(doc.status.as_str())
        .bind(doc.pages as i32)
        .bind(doc.chunk_count as i32)
        .bind(&doc.error)
        .bind(doc.created_at)
        .bind(doc.updated_at)
        .execute(&self.pool)
        .await
        .map_err(map_err)?;
        Ok(())
    }

    async fn find(&self, id: Uuid) -> AppResult<Option<Document>> {
        let row = sqlx::query(
            "SELECT id, owner, filename, status, pages, chunk_count, error, created_at, updated_at \
             FROM documents WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_err)?;
        row.as_ref().map(row_to_document).transpose()
    }

    async fn update(&self, doc: Document) -> AppResult<()> {
        let res = sqlx::query(
            "UPDATE documents SET filename = $2, status = $3, pages = $4, chunk_count = $5, \
             error = $6, updated_at = $7 WHERE id = $1",
        )
        .bind(doc.id)
        .bind(&doc.filename)
        .bind(doc.status.as_str())
        .bind(doc.pages as i32)
        .bind(doc.chunk_count as i32)
        .bind(&doc.error)
        .bind(doc.updated_at)
        .execute(&self.pool)
        .await
        .map_err(map_err)?;
        if res.rows_affected() == 0 {
            return Err(AppError::NotFound(format!("Document {}", doc.id)));
        }
        Ok(())
    }

    async fn list(&self, owner: Option<&str>) -> AppResult<Vec<Document>> {
        let rows = match owner {
            Some(o) => sqlx::query(
                "SELECT id, owner, filename, status, pages, chunk_count, error, created_at, updated_at \
                 FROM documents WHERE owner = $1 ORDER BY created_at DESC LIMIT 1000",
            )
            .bind(o)
            .fetch_all(&self.pool)
            .await,
            None => sqlx::query(
                "SELECT id, owner, filename, status, pages, chunk_count, error, created_at, updated_at \
                 FROM documents ORDER BY created_at DESC LIMIT 1000",
            )
            .fetch_all(&self.pool)
            .await,
        }
        .map_err(map_err)?;

        Ok(rows
            .iter()
            .filter_map(|row| match row_to_document(row) {
                Ok(d) => Some(d),
                Err(e) => {
                    tracing::warn!("skipping unreadable document row: {e}");
                    None
                }
            })
            .collect())
    }

    async fn delete(&self, id: Uuid) -> AppResult<()> {
        sqlx::query("DELETE FROM documents WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(map_err)?;
        Ok(())
    }
}

// ── Chunks ───────────────────────────────────────────────────────────────────

pub struct PgChunkRepository {
    pool: PgPool,
}

impl PgChunkRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ChunkRepository for PgChunkRepository {
    async fn insert_many(&self, chunks: Vec<Chunk>) -> AppResult<()> {
        if chunks.is_empty() {
            return Ok(());
        }
        let mut tx = self.pool.begin().await.map_err(map_err)?;
        for chunk in &chunks {
            sqlx::query(
                "INSERT INTO chunks (id, document_id, idx, text, char_start, char_end) \
                 VALUES ($1, $2, $3, $4, $5, $6)",
            )
            .bind(chunk.id)
            .bind(chunk.document_id)
            .bind(chunk.index as i32)
            .bind(&chunk.text)
            .bind(chunk.char_start as i64)
            .bind(chunk.char_end as i64)
            .execute(&mut *tx)
            .await
            .map_err(map_err)?;
        }
        tx.commit().await.map_err(map_err)?;
        Ok(())
    }

    async fn list_for_document(&self, document_id: Uuid) -> AppResult<Vec<Chunk>> {
        let rows = sqlx::query(
            "SELECT id, document_id, idx, text, char_start, char_end FROM chunks \
             WHERE document_id = $1 ORDER BY idx ASC",
        )
        .bind(document_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;

        rows.iter()
            .map(|row| {
                Ok(Chunk {
                    id: row.try_get("id").map_err(map_err)?,
                    document_id: row.try_get("document_id").map_err(map_err)?,
                    index: row.try_get::<i32, _>("idx").map_err(map_err)?.max(0) as u32,
                    text: row.try_get("text").map_err(map_err)?,
                    char_start: row.try_get::<i64, _>("char_start").map_err(map_err)?.max(0)
                        as usize,
                    char_end: row.try_get::<i64, _>("char_end").map_err(map_err)?.max(0) as usize,
                })
            })
            .collect()
    }

    async fn delete_for_document(&self, document_id: Uuid) -> AppResult<()> {
        sqlx::query("DELETE FROM chunks WHERE document_id = $1")
            .bind(document_id)
            .execute(&self.pool)
            .await
            .map_err(map_err)?;
        Ok(())
    }
}
