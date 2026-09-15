//! `/api/v1/jobs` — ingestion/query job history for the admin UI (session +
//! admin role). Read-only: RAG jobs are created by `/api/rag/*` itself, not
//! from the panel the way a spreadsheet batch job once was.

use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    Json,
    extract::{Path as UrlPath, Query, State},
};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    domain::processing::entities::job_stats,
    errors::{AppError, AppResult},
    infrastructure::http::{
        middleware::session::AdminUser,
        pagination::{PageParams, envelope},
    },
    state::AppState,
};

/// GET /api/v1/jobs
pub async fn list(
    _admin: AdminUser,
    State(state): State<Arc<AppState>>,
    Query(q): Query<HashMap<String, String>>,
) -> AppResult<Json<Value>> {
    let params = PageParams::from_query(&q);
    let all = state.jobs.list().await?;

    let mut jobs = all.clone();
    if let Some(s) = &params.search {
        jobs.retain(|j| {
            j.label
                .as_deref()
                .is_some_and(|l| l.to_lowercase().contains(s))
                || j.id.to_string().contains(s)
                || j.results.iter().any(|r| r.file.to_lowercase().contains(s))
        });
    }
    if let Some(status) = q
        .get("status")
        .filter(|s| !s.is_empty() && s.as_str() != "all")
    {
        jobs.retain(|j| j.status.as_str() == status);
    }
    if let Some(origin) = q
        .get("origin")
        .filter(|s| !s.is_empty() && s.as_str() != "all")
    {
        jobs.retain(|j| j.origin.as_str() == origin);
    }
    if let Some(operation) = q
        .get("operation")
        .filter(|s| !s.is_empty() && s.as_str() != "all")
    {
        jobs.retain(|j| &j.operation == operation);
    }

    let rows: Vec<Value> = jobs.iter().map(|j| j.to_summary_json()).collect();
    let mut env = envelope(rows, &params);
    env["stats"] = job_stats(&all);
    Ok(Json(env))
}

/// GET /api/v1/jobs/:id
pub async fn show(
    _admin: AdminUser,
    State(state): State<Arc<AppState>>,
    UrlPath(id): UrlPath<Uuid>,
) -> AppResult<Json<Value>> {
    let job = state
        .jobs
        .find(id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Job {id}")))?;
    Ok(Json(job.to_detail_json()))
}

/// GET /api/v1/jobs/stats — quick counts for the dashboard.
pub async fn stats(
    _admin: AdminUser,
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<Value>> {
    let all = state.jobs.list().await?;
    Ok(Json(job_stats(&all)))
}
