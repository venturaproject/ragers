//! `/api/chat/*` — persisted threads around `POST /api/rag/query`. Thread
//! CRUD is plain persistence (works with the in-memory stores the test
//! harness uses, no `LLM_API_KEY` needed); only `POST .../messages` calls
//! into RAG and is unavailable (503) in this harness — same convention as
//! `tests/rag.rs`.
#![allow(clippy::unwrap_used)]

mod common;

use axum::http::StatusCode;
use common::TestApp;
use serde_json::json;

#[tokio::test]
async fn thread_endpoints_require_authentication() {
    let mut app = TestApp::new();
    assert_eq!(
        app.post_json("/api/chat/threads", json!({})).await.status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        app.get("/api/chat/threads").await.status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        app.get("/api/chat/threads/00000000-0000-0000-0000-000000000000")
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn create_list_rename_and_delete_a_thread() {
    let mut app = TestApp::new();

    let created = app
        .post_json_key("/api/chat/threads", "test-key", json!({ "title": "Hilo 1" }))
        .await;
    assert_eq!(created.status, StatusCode::OK);
    let id = created.json["id"].as_str().unwrap().to_string();

    let listed = app.get_key("/api/chat/threads", "test-key").await;
    assert_eq!(listed.status, StatusCode::OK);
    assert_eq!(listed.json.as_array().unwrap().len(), 1);

    let renamed = app
        .json_with(
            "PATCH",
            &format!("/api/chat/threads/{id}"),
            &[("x-api-key", "test-key")],
            json!({ "title": "Renombrado" }),
        )
        .await;
    assert_eq!(renamed.status, StatusCode::OK);
    assert_eq!(renamed.json["title"], "Renombrado");

    let detail = app
        .get_key(&format!("/api/chat/threads/{id}"), "test-key")
        .await;
    assert_eq!(detail.status, StatusCode::OK);
    assert_eq!(detail.json["messages"].as_array().unwrap().len(), 0);

    let deleted = app
        .json_with(
            "DELETE",
            &format!("/api/chat/threads/{id}"),
            &[("x-api-key", "test-key")],
            json!({}),
        )
        .await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT);

    let after_delete = app
        .get_key(&format!("/api/chat/threads/{id}"), "test-key")
        .await;
    assert_eq!(after_delete.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn asking_in_a_thread_is_unavailable_without_configuration() {
    let mut app = TestApp::new();

    let created = app
        .post_json_key("/api/chat/threads", "test-key", json!({}))
        .await;
    let id = created.json["id"].as_str().unwrap().to_string();

    let res = app
        .post_json_key(
            &format!("/api/chat/threads/{id}/messages"),
            "test-key",
            json!({ "question": "what's in here?" }),
        )
        .await;
    assert_eq!(res.status, StatusCode::SERVICE_UNAVAILABLE);
}
