//! `/api/rag/*` — ingest a PDF into the vector index, list/inspect/delete
//! what's indexed, and ask grounded questions over it. Answers 503 when
//! `state.rag` is `None` (`LLM_API_KEY` unset, or Qdrant wasn't reachable at
//! startup) rather than half-working.

use std::sync::Arc;
use std::time::Instant;

use axum::{
    Json,
    extract::{Multipart, Path, Query, State},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    application::rag::service,
    domain::{processing::entities::Timings, rag::entities::Document},
    errors::{AppError, AppResult},
    infrastructure::http::{
        handlers::process_common::{
            authorize, check_ai_quota, read_multipart, record_ai_usage, trace_sync,
        },
        middleware::api_key::ApiPrincipal,
    },
    state::AppState,
};

/// Fetch a document the caller is allowed to see, or a 404 that doesn't
/// confirm the id exists for another tenant — same convention as jobs.
/// Best-effort: re-read the chunks that were just stored and hand them to
/// the full-text index. A failure here never fails the ingest response —
/// the chunk text is already durable in `chunks`/Qdrant either way.
async fn index_for_search(state: &AppState, doc: &Document) {
    let Ok(chunks) = state.chunks.list_for_document(doc.id).await else {
        return;
    };
    for chunk in &chunks {
        let _ = state.search.index_chunk(doc.owner.as_deref(), doc.id, chunk).await;
    }
    let _ = state.search.commit().await;
}

async fn owned_document(
    state: &AppState,
    principal: &ApiPrincipal,
    id: Uuid,
) -> AppResult<Document> {
    let doc = state
        .documents
        .find(id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Document {id}")))?;
    let owned =
        principal.is_privileged() || (doc.owner.is_some() && doc.owner == principal.owner_key());
    if !owned {
        return Err(AppError::NotFound(format!("Document {id}")));
    }
    Ok(doc)
}

/// POST /api/rag/documents  (multipart: `file`, a PDF; optional `redact_pii`
/// field — "true"/"1" — to strip recognised PII before it's chunked/embedded)
#[utoipa::path(
    post, path = "/api/rag/documents", tag = "rag",
    request_body(content = crate::infrastructure::http::openapi::UploadForm, content_type = "multipart/form-data"),
    responses(
        (status = 200, description = "The ingested document (status `ready`, or `failed` with `error` set)"),
        (status = 400, description = "Not a readable PDF, or it has no extractable text"),
        (status = 401, description = "Missing or invalid credentials"),
        (status = 403, description = "Token lacks the `rag:ingest` scope"),
        (status = 503, description = "RAG is not configured on this server (`LLM_API_KEY` unset, or Qdrant unreachable)"),
    ),
    security(("api_key" = []), ("bearer" = [])),
)]
pub async fn ingest(
    State(state): State<Arc<AppState>>,
    principal: ApiPrincipal,
    multipart: Multipart,
) -> AppResult<Json<serde_json::Value>> {
    // Scope enforcement runs before the "is RAG even configured" gate, same
    // as any other auth check — a caller without the right scope shouldn't
    // learn whether the feature is turned on.
    authorize(&state, &principal, "rag:ingest", true).await?;
    let Some(engine) = state.rag.as_ref() else {
        return Err(AppError::ServiceUnavailable(
            "RAG is not configured on this server".into(),
        ));
    };
    check_ai_quota(&state, &principal).await?;

    let mp = read_multipart(&state, multipart).await?;
    let (fname, bytes) = mp
        .file("file")
        .ok_or_else(|| AppError::BadRequest("missing `file` field".into()))?;
    let filename = fname.to_string();
    let bytes = bytes.to_vec();
    let redact_pii = mp
        .field_string("redact_pii")
        .is_some_and(|v| matches!(v.trim().to_lowercase().as_str(), "1" | "true" | "yes"));

    let doc = Document::new(principal.owner_key(), filename.clone());
    state.documents.create(doc.clone()).await?;

    let started = Instant::now();
    let result = service::ingest_pdf(
        engine,
        &state.documents,
        &state.chunks,
        doc,
        &bytes,
        redact_pii,
    )
    .await;

    let (outcome, tokens) = match &result {
        Ok((doc, tokens, _redacted)) => (
            Ok((doc.chunk_count as u64, 0u32, elapsed(started))),
            *tokens,
        ),
        Err(e) => (Err((e.to_string(), started.elapsed().as_millis())), 0),
    };
    trace_sync(&state, filename, "rag_ingest", &principal, outcome).await;
    if tokens > 0 {
        record_ai_usage(&state, &principal, tokens as u64).await;
    }

    let (doc, _tokens, redacted_count) = result?;
    index_for_search(&state, &doc).await;
    let mut json = doc.to_json();
    json["pii_redacted"] = serde_json::json!(redacted_count);
    Ok(Json(json))
}

fn elapsed(started: Instant) -> Timings {
    let ms = started.elapsed().as_millis();
    Timings {
        parse_ms: ms,
        total_ms: ms,
        ..Default::default()
    }
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    /// Admin-only: list every caller's documents, not just your own.
    #[serde(default)]
    pub all: bool,
}

/// GET /api/rag/documents
#[utoipa::path(
    get, path = "/api/rag/documents", tag = "rag",
    responses((status = 200, description = "Documents this caller has ingested, newest first")),
    security(("api_key" = []), ("bearer" = [])),
)]
pub async fn list(
    State(state): State<Arc<AppState>>,
    principal: ApiPrincipal,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Vec<serde_json::Value>>> {
    authorize(&state, &principal, "rag:ingest", false).await?;
    let owner = if q.all && principal.is_privileged() {
        None
    } else {
        principal.owner_key()
    };
    let docs = state.documents.list(owner.as_deref()).await?;
    Ok(Json(docs.iter().map(Document::to_json).collect()))
}

/// GET /api/rag/documents/:id
#[utoipa::path(
    get, path = "/api/rag/documents/{id}", tag = "rag",
    responses(
        (status = 200, description = "Document detail, plus its chunks"),
        (status = 404, description = "No such document, or it belongs to another caller"),
    ),
    security(("api_key" = []), ("bearer" = [])),
)]
pub async fn detail(
    State(state): State<Arc<AppState>>,
    principal: ApiPrincipal,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    authorize(&state, &principal, "rag:ingest", false).await?;
    let doc = owned_document(&state, &principal, id).await?;
    let chunks = state.chunks.list_for_document(id).await?;
    let mut json = doc.to_json();
    json["chunks"] = serde_json::json!(
        chunks
            .iter()
            .map(|c| serde_json::json!({
                "index": c.index,
                "text": c.text,
                "char_start": c.char_start,
                "char_end": c.char_end,
            }))
            .collect::<Vec<_>>()
    );
    Ok(Json(json))
}

/// DELETE /api/rag/documents/:id
#[utoipa::path(
    delete, path = "/api/rag/documents/{id}", tag = "rag",
    responses(
        (status = 204, description = "Deleted — its chunks and vectors are gone too"),
        (status = 404, description = "No such document, or it belongs to another caller"),
    ),
    security(("api_key" = []), ("bearer" = [])),
)]
pub async fn delete(
    State(state): State<Arc<AppState>>,
    principal: ApiPrincipal,
    Path(id): Path<Uuid>,
) -> AppResult<axum::http::StatusCode> {
    authorize(&state, &principal, "rag:ingest", true).await?;
    owned_document(&state, &principal, id).await?;

    if let Some(engine) = state.rag.as_ref() {
        engine.vector_store.delete_document(id).await?;
    }
    let _ = state.search.delete_document(id).await;
    let _ = state.search.commit().await;
    state.chunks.delete_for_document(id).await?;
    state.documents.delete(id).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct QueryBody {
    pub question: String,
    pub top_k: Option<u64>,
    pub document_ids: Option<Vec<Uuid>>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct QuerySource {
    pub document_id: Uuid,
    pub chunk_index: u32,
    pub text: String,
    pub score: f32,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct QueryResult {
    pub answer: String,
    pub sources: Vec<QuerySource>,
}

/// POST /api/rag/query
#[utoipa::path(
    post, path = "/api/rag/query", tag = "rag",
    request_body = crate::infrastructure::http::openapi::RagQueryBody,
    responses(
        (status = 200, description = "Grounded answer + the chunks it was built from", body = QueryResult),
        (status = 400, description = "Empty question"),
        (status = 401, description = "Missing or invalid credentials"),
        (status = 403, description = "Token lacks the `rag:query` scope"),
        (status = 503, description = "RAG is not configured on this server"),
    ),
    security(("api_key" = []), ("bearer" = [])),
)]
pub async fn query(
    State(state): State<Arc<AppState>>,
    principal: ApiPrincipal,
    Json(body): Json<QueryBody>,
) -> AppResult<Json<QueryResult>> {
    authorize(&state, &principal, "rag:query", true).await?;
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

    let started = Instant::now();
    let result = service::answer_question(
        engine,
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
    trace_sync(
        &state,
        body.question.clone(),
        "rag_query",
        &principal,
        outcome,
    )
    .await;
    if tokens > 0 {
        record_ai_usage(&state, &principal, tokens as u64).await;
    }

    let out = result?;
    Ok(Json(QueryResult {
        answer: out.answer,
        sources: out
            .sources
            .into_iter()
            .map(|s| QuerySource {
                document_id: s.document_id,
                chunk_index: s.chunk_index,
                text: s.text,
                score: s.score,
            })
            .collect(),
    }))
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ErpSyncItem {
    pub title: String,
    pub status: String,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ErpSyncResult {
    pub ingested: u32,
    pub failed: u32,
    pub pii_redacted: u64,
    pub items: Vec<ErpSyncItem>,
}

#[derive(Debug, Deserialize)]
pub struct SyncErpQuery {
    /// Strip recognised PII (emails, IDs, cards, phone numbers) from each
    /// record's text before it's chunked/embedded. Off by default.
    #[serde(default)]
    pub redact_pii: bool,
}

/// POST /api/rag/sync/erp — pull products + orders from the external
/// ERP/CRM database (`ERP_DATABASE_URL`) and ingest each as a RAG document,
/// through the same chunk/embed/store pipeline as an uploaded PDF.
///
/// Idempotent by `title`: a record that was already ingested by a previous
/// sync has its old document (and vectors/chunks) deleted first, then
/// re-ingested fresh — re-running the sync updates in place instead of
/// piling up duplicate documents that would otherwise dilute retrieval
/// scores (and burn embedding tokens) a little more on every click.
#[utoipa::path(
    post, path = "/api/rag/sync/erp", tag = "rag",
    responses(
        (status = 200, description = "Sync summary: how many ERP records were ingested", body = ErpSyncResult),
        (status = 401, description = "Missing or invalid credentials"),
        (status = 403, description = "Token lacks the `rag:ingest` scope"),
        (status = 503, description = "RAG or the ERP source is not configured on this server"),
    ),
    security(("api_key" = []), ("bearer" = [])),
)]

pub async fn sync_erp(
    State(state): State<Arc<AppState>>,
    principal: ApiPrincipal,
    Query(q): Query<SyncErpQuery>,
) -> AppResult<Json<ErpSyncResult>> {
    authorize(&state, &principal, "rag:ingest", true).await?;
    let Some(engine) = state.rag.as_ref() else {
        return Err(AppError::ServiceUnavailable(
            "RAG is not configured on this server".into(),
        ));
    };
    let Some(erp_pool) = state.erp_pool.as_ref() else {
        return Err(AppError::ServiceUnavailable(
            "No ERP_DATABASE_URL configured on this server".into(),
        ));
    };
    check_ai_quota(&state, &principal).await?;

    let mut records = crate::application::rag::erp_sync::fetch_products(erp_pool).await?;
    if let Some(catalog) = crate::application::rag::erp_sync::fetch_catalog_summary(erp_pool).await? {
        records.push(catalog);
    }
    records.extend(crate::application::rag::erp_sync::fetch_orders(erp_pool).await?);

    // Existing documents from a prior sync, keyed by filename, so a record
    // that was already ingested gets replaced rather than duplicated.
    let existing = state.documents.list(principal.owner_key().as_deref()).await?;
    let existing_by_title: std::collections::HashMap<String, Uuid> = existing
        .into_iter()
        .map(|d| (d.filename.clone(), d.id))
        .collect();

    let mut ingested = 0u32;
    let mut failed = 0u32;
    let mut items = Vec::with_capacity(records.len());
    let mut total_tokens: u64 = 0;
    let mut total_redacted: u64 = 0;

    for record in records {
        if let Some(&old_id) = existing_by_title.get(&record.title) {
            engine.vector_store.delete_document(old_id).await?;
            let _ = state.search.delete_document(old_id).await;
            state.chunks.delete_for_document(old_id).await?;
            state.documents.delete(old_id).await?;
        }

        let doc = Document::new(principal.owner_key(), record.title.clone());
        state.documents.create(doc.clone()).await?;

        let started = Instant::now();
        let result = service::ingest_text(
            engine,
            &state.documents,
            &state.chunks,
            doc,
            &record.text,
            1,
            q.redact_pii,
        )
        .await;

        let (outcome, tokens) = match &result {
            Ok((doc, tokens, _redacted)) => (
                Ok((doc.chunk_count as u64, 0u32, elapsed(started))),
                *tokens,
            ),
            Err(e) => (Err((e.to_string(), started.elapsed().as_millis())), 0),
        };
        trace_sync(&state, record.title.clone(), "rag_erp_sync", &principal, outcome).await;

        match result {
            Ok((ref doc, _, redacted)) => {
                ingested += 1;
                total_tokens += tokens as u64;
                total_redacted += redacted as u64;
                index_for_search(&state, doc).await;
                items.push(ErpSyncItem {
                    title: record.title,
                    status: "ready".into(),
                    error: None,
                });
            }
            Err(e) => {
                failed += 1;
                items.push(ErpSyncItem {
                    title: record.title,
                    status: "failed".into(),
                    error: Some(e.to_string()),
                });
            }
        }
    }

    if total_tokens > 0 {
        record_ai_usage(&state, &principal, total_tokens).await;
    }

    Ok(Json(ErpSyncResult {
        ingested,
        failed,
        pii_redacted: total_redacted,
        items,
    }))
}
