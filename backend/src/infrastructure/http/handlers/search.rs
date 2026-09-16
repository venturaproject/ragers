//! `GET /api/search` — BM25 keyword search over ingested chunks, no LLM
//! call involved. See `application::search` for the index itself.

use std::sync::Arc;

use axum::{Json, extract::{Query, State}};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    errors::AppResult,
    infrastructure::http::{handlers::process_common::authorize, middleware::api_key::ApiPrincipal},
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: String,
    #[serde(default)]
    pub limit: Option<usize>,
    /// Admin-only: search every caller's documents, not just your own.
    #[serde(default)]
    pub all: bool,
}

/// GET /api/search?q=...&limit=10
pub async fn search(
    State(state): State<Arc<AppState>>,
    principal: ApiPrincipal,
    Query(q): Query<SearchQuery>,
) -> AppResult<Json<Value>> {
    authorize(&state, &principal, "rag:query", false).await?;

    let owner = if q.all && principal.is_privileged() {
        None
    } else {
        principal.owner_key()
    };
    let limit = q.limit.unwrap_or(10).clamp(1, 50);

    let hits = state.search.search(owner.as_deref(), &q.q, limit)?;

    Ok(Json(json!({
        "hits": hits.into_iter().map(|h| json!({
            "document_id": h.document_id,
            "chunk_index": h.chunk_index,
            "text": h.text,
            "score": h.score,
        })).collect::<Vec<_>>(),
    })))
}
