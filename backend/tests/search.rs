//! `GET /api/search` — BM25 keyword search, no `LLM_API_KEY` needed (unlike
//! `/api/rag/query`, this doesn't touch the RAG engine at all).
#![allow(clippy::unwrap_used)]

mod common;

use axum::http::StatusCode;
use common::TestApp;

#[tokio::test]
async fn search_requires_authentication() {
    let mut app = TestApp::new();
    assert_eq!(
        app.get("/api/search?q=anything").await.status,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn search_works_without_llm_configuration() {
    // No LLM_API_KEY in the test harness — search must still respond (it's
    // plain keyword lookup over whatever's indexed, no LLM/Qdrant call).
    let mut app = TestApp::new();
    let res = app.get_key("/api/search?q=nothing+indexed+yet", "test-key").await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.json["hits"].as_array().unwrap().len(), 0);
}
