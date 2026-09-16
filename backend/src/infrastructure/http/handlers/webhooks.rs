//! `/api/webhooks/dispatch` — POST the same JSON payload to many URLs
//! concurrently, each independently timed out and retried. See
//! `application::webhooks::service` for the fan-out + SSRF-guard details.

use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::{Json, extract::State};
use serde::Deserialize;
use serde_json::json;

use crate::{
    application::webhooks::service::{self, MAX_TARGETS},
    errors::{AppError, AppResult},
    infrastructure::http::{
        handlers::process_common::{authorize, trace_sync},
        middleware::api_key::ApiPrincipal,
    },
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct DispatchBody {
    /// Up to [`MAX_TARGETS`] URLs. `http`/`https` only; a URL that resolves
    /// to a literal loopback/private/link-local address, or one of this
    /// deployment's own internal service names, is rejected per-target
    /// rather than failing the whole batch.
    pub targets: Vec<String>,
    pub payload: serde_json::Value,
    /// Per-attempt timeout in seconds. Default 10, capped at 60.
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    /// Retries after the first attempt, exponential backoff between them.
    /// Default 2, capped at 5.
    #[serde(default)]
    pub max_retries: Option<u32>,
}

/// POST /api/webhooks/dispatch
#[utoipa::path(
    post, path = "/api/webhooks/dispatch", tag = "webhooks",
    request_body = crate::infrastructure::http::openapi::WebhookDispatchBody,
    responses(
        (status = 200, description = "Per-target results — a non-2xx or unreachable target is reported, not an error for the whole batch"),
        (status = 400, description = "No targets, or more than the maximum allowed"),
        (status = 401, description = "Missing or invalid credentials"),
        (status = 403, description = "Token lacks the `webhooks:dispatch` scope"),
    ),
    security(("api_key" = []), ("bearer" = [])),
)]
pub async fn dispatch(
    State(state): State<Arc<AppState>>,
    principal: ApiPrincipal,
    Json(body): Json<DispatchBody>,
) -> AppResult<Json<serde_json::Value>> {
    authorize(&state, &principal, "webhooks:dispatch", true).await?;

    if body.targets.is_empty() {
        return Err(AppError::BadRequest("targets must not be empty".into()));
    }
    if body.targets.len() > MAX_TARGETS {
        return Err(AppError::BadRequest(format!(
            "too many targets ({}); the limit is {MAX_TARGETS}",
            body.targets.len()
        )));
    }

    let timeout = Duration::from_secs(body.timeout_secs.unwrap_or(10).clamp(1, 60));
    let max_retries = body.max_retries.unwrap_or(2).min(5);

    let started = Instant::now();
    let results = service::dispatch(
        &state.webhook_client,
        body.targets.clone(),
        &body.payload,
        timeout,
        max_retries,
    )
    .await;

    let succeeded = results.iter().filter(|r| r.ok).count() as u64;
    let failed = results.len() as u64 - succeeded;

    trace_sync(
        &state,
        format!("{} target(s)", body.targets.len()),
        "webhooks_dispatch",
        &principal,
        Ok((
            succeeded,
            failed as u32,
            crate::domain::processing::entities::Timings {
                total_ms: started.elapsed().as_millis(),
                ..Default::default()
            },
        )),
    )
    .await;

    Ok(Json(json!({
        "succeeded": succeeded,
        "failed": failed,
        "results": results,
    })))
}
