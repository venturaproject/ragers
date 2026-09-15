#![allow(clippy::unwrap_used)]
mod common;

use axum::http::StatusCode;
use common::TestApp;
use serde_json::json;

#[tokio::test]
async fn security_headers_on_every_response() {
    let mut app = TestApp::new();
    let r = app.get("/api/v1/config").await;
    for h in [
        "x-content-type-options",
        "x-frame-options",
        "referrer-policy",
        "cross-origin-opener-policy",
        "x-permitted-cross-domain-policies",
    ] {
        assert!(r.headers.contains_key(h), "missing {h}");
    }
    assert_eq!(r.headers["x-content-type-options"], "nosniff");
    assert_eq!(r.headers["x-frame-options"], "DENY");
    // not HTTPS in tests → no HSTS
    assert!(!r.headers.contains_key("strict-transport-security"));
}

#[tokio::test]
async fn invalid_api_key_is_rejected() {
    let mut app = TestApp::new();
    let r = app
        .post_json_key(
            "/api/rag/query",
            "not-a-real-key",
            json!({ "question": "anything" }),
        )
        .await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
}
