//! `POST /api/webhooks/dispatch` — auth, input validation, and the SSRF
//! guard exercised end-to-end through the HTTP layer (the retry/backoff and
//! success-path logic are unit-tested directly in
//! `application::webhooks::service`, since a real reachable non-loopback
//! target isn't available in this harness).
#![allow(clippy::unwrap_used)]

mod common;

use axum::http::StatusCode;
use common::TestApp;
use serde_json::json;

#[tokio::test]
async fn dispatch_requires_authentication() {
    let mut app = TestApp::new();
    let res = app
        .post_json(
            "/api/webhooks/dispatch",
            json!({ "targets": ["https://example.com"], "payload": {} }),
        )
        .await;
    assert_eq!(res.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn rejects_an_empty_target_list() {
    let mut app = TestApp::new();
    let res = app
        .post_json_key(
            "/api/webhooks/dispatch",
            "test-key",
            json!({ "targets": [], "payload": {} }),
        )
        .await;
    assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn rejects_more_targets_than_the_limit() {
    let mut app = TestApp::new();
    let too_many: Vec<String> = (0..300).map(|i| format!("https://example.com/{i}")).collect();
    let res = app
        .post_json_key(
            "/api/webhooks/dispatch",
            "test-key",
            json!({ "targets": too_many, "payload": {} }),
        )
        .await;
    assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn a_loopback_target_is_reported_as_failed_not_dispatched() {
    let mut app = TestApp::new();
    let res = app
        .post_json_key(
            "/api/webhooks/dispatch",
            "test-key",
            json!({ "targets": ["http://127.0.0.1:1/hook"], "payload": {"hello": "world"} }),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.json["succeeded"], 0);
    assert_eq!(res.json["failed"], 1);
    let results = res.json["results"].as_array().unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["ok"], false);
    assert_eq!(results[0]["attempts"], 0);
}
