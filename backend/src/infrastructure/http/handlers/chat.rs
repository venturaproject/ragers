//! `/api/chat/*` — persisted threads around `POST /api/rag/query`, so a
//! caller can come back and see what they asked. Each question is still
//! answered independently (see `application::chat::service`) — this only
//! adds a record of the conversation, not memory for the model.

use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    application::chat::service,
    domain::chat::entities::ChatThread,
    errors::{AppError, AppResult},
    infrastructure::http::{
        handlers::process_common::{authorize, check_ai_quota, record_ai_usage, trace_sync},
        middleware::api_key::ApiPrincipal,
    },
    state::AppState,
};

async fn owned_thread(
    state: &AppState,
    principal: &ApiPrincipal,
    id: Uuid,
) -> AppResult<ChatThread> {
    let thread = state
        .chat_threads
        .find(id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Chat thread {id}")))?;
    let owned = principal.is_privileged()
        || (thread.owner.is_some() && thread.owner == principal.owner_key());
    if !owned {
        return Err(AppError::NotFound(format!("Chat thread {id}")));
    }
    Ok(thread)
}

#[derive(Debug, Deserialize)]
pub struct CreateThreadBody {
    #[serde(default)]
    pub title: Option<String>,
}

/// POST /api/chat/threads
pub async fn create(
    State(state): State<Arc<AppState>>,
    principal: ApiPrincipal,
    Json(body): Json<CreateThreadBody>,
) -> AppResult<Json<Value>> {
    authorize(&state, &principal, "rag:query", true).await?;
    let title = body
        .title
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| "Nueva conversación".to_string());
    let thread = ChatThread::new(principal.owner_key(), title);
    state.chat_threads.create(thread.clone()).await?;
    Ok(Json(thread.to_json()))
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    /// Admin-only: list every caller's threads, not just your own.
    #[serde(default)]
    pub all: bool,
}

/// GET /api/chat/threads
pub async fn list(
    State(state): State<Arc<AppState>>,
    principal: ApiPrincipal,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Vec<Value>>> {
    authorize(&state, &principal, "rag:query", false).await?;
    let owner = if q.all && principal.is_privileged() {
        None
    } else {
        principal.owner_key()
    };
    let threads = state.chat_threads.list(owner.as_deref()).await?;
    Ok(Json(threads.iter().map(ChatThread::to_json).collect()))
}

/// GET /api/chat/threads/:id — thread + its messages, oldest first.
pub async fn detail(
    State(state): State<Arc<AppState>>,
    principal: ApiPrincipal,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    authorize(&state, &principal, "rag:query", false).await?;
    let thread = owned_thread(&state, &principal, id).await?;
    let messages = state.chat_messages.list_for_thread(id).await?;
    let mut out = thread.to_json();
    out["messages"] = json!(messages.iter().map(|m| m.to_json()).collect::<Vec<_>>());
    Ok(Json(out))
}

#[derive(Debug, Deserialize)]
pub struct RenameThreadBody {
    pub title: String,
}

/// PATCH /api/chat/threads/:id
pub async fn rename(
    State(state): State<Arc<AppState>>,
    principal: ApiPrincipal,
    Path(id): Path<Uuid>,
    Json(body): Json<RenameThreadBody>,
) -> AppResult<Json<Value>> {
    authorize(&state, &principal, "rag:query", true).await?;
    let mut thread = owned_thread(&state, &principal, id).await?;
    let title = body.title.trim();
    if title.is_empty() {
        return Err(AppError::BadRequest("title must not be empty".into()));
    }
    state.chat_threads.rename(id, title.to_string()).await?;
    thread.title = title.to_string();
    Ok(Json(thread.to_json()))
}

/// DELETE /api/chat/threads/:id
pub async fn delete(
    State(state): State<Arc<AppState>>,
    principal: ApiPrincipal,
    Path(id): Path<Uuid>,
) -> AppResult<axum::http::StatusCode> {
    authorize(&state, &principal, "rag:query", true).await?;
    owned_thread(&state, &principal, id).await?;
    state.chat_messages.delete_for_thread(id).await?;
    state.chat_threads.delete(id).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct AskBody {
    pub question: String,
    pub top_k: Option<u64>,
    pub document_ids: Option<Vec<Uuid>>,
}

/// POST /api/chat/threads/:id/messages — ask a question inside a thread.
/// Persists the question + answer either way; a RAG-side failure (503,
/// empty question, …) still returns the error to the caller.
pub async fn ask(
    State(state): State<Arc<AppState>>,
    principal: ApiPrincipal,
    Path(id): Path<Uuid>,
    Json(body): Json<AskBody>,
) -> AppResult<Json<Value>> {
    authorize(&state, &principal, "rag:query", true).await?;
    let thread = owned_thread(&state, &principal, id).await?;
    let Some(engine) = state.rag.as_ref() else {
        return Err(AppError::ServiceUnavailable(
            "RAG is not configured on this server".into(),
        ));
    };
    check_ai_quota(&state, &principal).await?;

    if body.question.trim().is_empty() {
        return Err(AppError::BadRequest("question must not be empty".into()));
    }

    let owner = if principal.is_privileged() {
        None
    } else {
        principal.owner_key()
    };

    fn elapsed(started: std::time::Instant) -> crate::domain::processing::entities::Timings {
        let ms = started.elapsed().as_millis();
        crate::domain::processing::entities::Timings {
            parse_ms: ms,
            total_ms: ms,
            ..Default::default()
        }
    }

    let started = std::time::Instant::now();
    let result = service::ask_in_thread(
        engine,
        &state.chat_threads,
        &state.chat_messages,
        thread.id,
        owner.as_deref(),
        body.document_ids.as_deref(),
        &body.question,
        body.top_k,
    )
    .await;

    let (outcome, tokens) = match &result {
        Ok(out) => {
            let chat_tokens = out.usage.as_ref().map(|u| u.total_tokens).unwrap_or(0);
            (
                Ok((out.sources.len() as u64, 0u32, elapsed(started))),
                out.embedding_tokens + chat_tokens,
            )
        }
        Err(e) => (Err((e.to_string(), started.elapsed().as_millis())), 0),
    };
    trace_sync(&state, body.question.clone(), "chat_ask", &principal, outcome).await;
    if tokens > 0 {
        record_ai_usage(&state, &principal, tokens as u64).await;
    }

    let out = result?;
    Ok(Json(json!({
        "answer": out.answer,
        "sources": out.sources.into_iter().map(|s| json!({
            "document_id": s.document_id,
            "chunk_index": s.chunk_index,
            "text": s.text,
            "score": s.score,
        })).collect::<Vec<_>>(),
    })))
}
