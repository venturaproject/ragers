//! Admin job history + dashboard. Populating real `rag_ingest`/`rag_query`
//! jobs needs a live LLM + Qdrant, so this only covers what's reachable
//! without them: auth gating and the zero-activity baseline. The Job
//! aggregation logic itself (`job_stats`, dashboard rollups) is unaffected
//! by which operation produced the jobs — it was exercised against
//! spreadsheet-processing jobs in the sibling Filers project this was
//! forked from.
#![allow(clippy::unwrap_used)]
mod common;

use axum::http::StatusCode;
use common::{TestApp, png_bytes};

#[tokio::test]
async fn jobs_list_requires_admin() {
    let mut app = TestApp::new();
    assert_eq!(
        app.get("/api/v1/jobs").await.status,
        StatusCode::UNAUTHORIZED
    );
    app.login_user().await;
    assert_eq!(app.get("/api/v1/jobs").await.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn dashboard_reports_zero_activity_cleanly() {
    let mut app = TestApp::new();
    app.login_admin().await;
    let d = app.get("/api/v1/dashboard").await;
    assert_eq!(d.status, StatusCode::OK);
    assert_eq!(d.json["processings"]["total"], 0);
    assert_eq!(d.json["counts"]["users"], 3);
    assert_eq!(d.json["counts"]["roles"], 2);
    assert!(d.json["recent"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn avatar_upload() {
    let mut app = TestApp::new();
    app.login_admin().await;

    let ok = app
        .post_file(
            "/api/v1/auth/me/avatar",
            "avatar",
            "me.png",
            "image/png",
            &png_bytes(),
        )
        .await;
    assert_eq!(ok.status, StatusCode::OK);
    let avatar = ok.json["user"]["avatar"].as_str().unwrap();
    assert!(avatar.starts_with("data:image/png;base64,"));

    // persisted
    assert_eq!(
        app.get("/api/v1/auth/me").await.json["user"]["avatar"],
        avatar
    );

    // non-image rejected
    let bad = app
        .post_file(
            "/api/v1/auth/me/avatar",
            "avatar",
            "x.txt",
            "text/plain",
            b"hello",
        )
        .await;
    assert_eq!(bad.status, StatusCode::UNPROCESSABLE_ENTITY);
}
