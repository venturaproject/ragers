//! `/api/rag/*` — disabled (503) in the test harness, which runs without
//! `LLM_API_KEY`/Qdrant. A real ingest-then-query round trip needs both;
//! `application::rag::{chunker, service}`'s own tests (and the ones in
//! `application::api_client::service` for AI-token quota) cover the request
//! shaping and business logic without a network dependency. This just
//! proves the routes are wired, fail closed, and check scope before
//! configuration (a caller without the right scope shouldn't learn whether
//! the feature is even turned on).
#![allow(clippy::unwrap_used)]

mod common;

use axum::http::StatusCode;
use common::{TestApp, pdf_bytes};
use serde_json::json;

#[tokio::test]
async fn ingest_is_unavailable_without_configuration() {
    let mut app = TestApp::new();
    let res = app
        .post_file_key(
            "/api/rag/documents",
            "test-key",
            "file",
            "invoice.pdf",
            "application/pdf",
            &pdf_bytes(1, "hello"),
        )
        .await;
    assert_eq!(res.status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(res.text().to_lowercase().contains("not configured"));
}

#[tokio::test]
async fn ingest_requires_authentication() {
    let mut app = TestApp::new();
    let res = app
        .post_file(
            "/api/rag/documents",
            "file",
            "invoice.pdf",
            "application/pdf",
            &pdf_bytes(1, "hello"),
        )
        .await;
    assert_eq!(res.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn query_is_unavailable_without_configuration() {
    let mut app = TestApp::new();
    let res = app
        .post_json_key(
            "/api/rag/query",
            "test-key",
            json!({ "question": "what's in the invoice?" }),
        )
        .await;
    assert_eq!(res.status, StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn query_requires_authentication() {
    let mut app = TestApp::new();
    let res = app
        .post_json("/api/rag/query", json!({ "question": "anything" }))
        .await;
    assert_eq!(res.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn documents_list_requires_authentication() {
    let mut app = TestApp::new();
    assert_eq!(
        app.get("/api/rag/documents").await.status,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn erp_sync_requires_authentication() {
    let mut app = TestApp::new();
    assert_eq!(
        app.post_empty("/api/rag/sync/erp").await.status,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn erp_sync_is_unavailable_without_configuration() {
    let mut app = TestApp::new();
    // Neither LLM_API_KEY nor ERP_DATABASE_URL is set in the test harness —
    // the RAG-not-configured check runs first (same order as `ingest`/`query`).
    let res = app.post_json_key("/api/rag/sync/erp", "test-key", json!({})).await;
    assert_eq!(res.status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(res.text().to_lowercase().contains("not configured"));
}
