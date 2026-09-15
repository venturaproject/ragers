//! OAuth2 client-credentials flow + admin management of API clients.
//!
//! The test harness never has `LLM_API_KEY` set, so every authenticated call
//! into `/api/rag/*` ends at the 503 "not configured" gate rather than at
//! real work — scope enforcement runs *before* that gate (see
//! `handlers/rag.rs`), so a 503 here is the signal a correctly-scoped bearer
//! token got all the way through auth, the same way a 200 would elsewhere.
//! Monthly-quota exhaustion (which needs an actual successful call to
//! accrue usage) is covered at the service level instead — see
//! `application::api_client::service::tests`.
#![allow(clippy::unwrap_used)]
mod common;

use axum::http::StatusCode;
use common::TestApp;
use serde_json::json;

async fn new_client(app: &mut TestApp, scopes: serde_json::Value) -> (String, String, String) {
    let r = app
        .post_json(
            "/api/v1/api-clients",
            json!({ "name": "ERP", "scopes": scopes }),
        )
        .await;
    assert_eq!(r.status, StatusCode::CREATED);
    (
        r.json["client"]["id"].as_str().unwrap().to_string(),
        r.json["client"]["client_id"].as_str().unwrap().to_string(),
        r.json["secret"].as_str().unwrap().to_string(),
    )
}

async fn token(app: &mut TestApp, client_id: &str, secret: &str) -> Resp2 {
    let r = app
        .post_json(
            "/api/ext/auth/token",
            json!({ "client_id": client_id, "client_secret": secret }),
        )
        .await;
    Resp2 {
        status: r.status,
        access: r.json["access_token"].as_str().map(str::to_string),
        refresh: r.json["refresh_token"].as_str().map(str::to_string),
    }
}

struct Resp2 {
    status: StatusCode,
    access: Option<String>,
    refresh: Option<String>,
}

#[tokio::test]
async fn admin_manages_clients_only_with_session() {
    let mut app = TestApp::new();
    assert_eq!(
        app.get("/api/v1/api-clients").await.status,
        StatusCode::UNAUTHORIZED
    );
    app.login_user().await;
    assert_eq!(
        app.get("/api/v1/api-clients").await.status,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn client_credentials_flow() {
    let mut app = TestApp::new();
    app.login_admin().await;
    let (_id, client_id, secret) = new_client(&mut app, json!(["rag:ingest", "rag:query"])).await;
    app.logout_local();

    // wrong secret
    assert_eq!(
        token(&mut app, &client_id, "nope").await.status,
        StatusCode::UNAUTHORIZED
    );

    let grant = token(&mut app, &client_id, &secret).await;
    assert_eq!(grant.status, StatusCode::OK);
    let access = grant.access.unwrap();

    // A correctly-scoped bearer token clears auth and reaches the "RAG not
    // configured" gate — 503, not 401/403.
    let r = app
        .post_json_bearer("/api/rag/query", &access, json!({ "question": "anything" }))
        .await;
    assert_eq!(r.status, StatusCode::SERVICE_UNAVAILABLE);

    // bad bearer → 401
    assert_eq!(
        app.post_json_bearer(
            "/api/rag/query",
            "deadbeef",
            json!({ "question": "anything" })
        )
        .await
        .status,
        StatusCode::UNAUTHORIZED
    );

    // refresh is single-use
    let refresh = grant.refresh.unwrap();
    let first = app
        .post_json("/api/ext/auth/refresh", json!({ "refresh_token": refresh }))
        .await;
    assert_eq!(first.status, StatusCode::OK);
    let rotated_access = first.json["access_token"].as_str().unwrap().to_string();
    let rotated_refresh = first.json["refresh_token"].as_str().unwrap().to_string();

    // Replaying the spent refresh token is treated as theft: 401 …
    let reused = app
        .post_json("/api/ext/auth/refresh", json!({ "refresh_token": refresh }))
        .await;
    assert_eq!(reused.status, StatusCode::UNAUTHORIZED);

    // … and the whole token family is revoked — the access + refresh that the
    // legitimate rotation just produced no longer work.
    assert_eq!(
        app.post_json_bearer(
            "/api/rag/query",
            &rotated_access,
            json!({ "question": "x" })
        )
        .await
        .status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        app.post_json(
            "/api/ext/auth/refresh",
            json!({ "refresh_token": rotated_refresh })
        )
        .await
        .status,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn scope_is_enforced() {
    let mut app = TestApp::new();
    app.login_admin().await;
    let (_id, client_id, secret) = new_client(&mut app, json!(["rag:query"])).await;
    app.logout_local();
    let access = token(&mut app, &client_id, &secret).await.access.unwrap();

    // rag:query cannot ingest
    assert_eq!(
        app.post_file_bearer(
            "/api/rag/documents",
            &access,
            "file",
            "doc.pdf",
            "application/pdf",
            b"%PDF-1.4\n",
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn rotate_invalidates_tokens_and_revoke_soft_deletes() {
    let mut app = TestApp::new();
    app.login_admin().await;
    let (id, client_id, secret) = new_client(&mut app, json!(["*"])).await;
    let access = {
        app.logout_local();
        let a = token(&mut app, &client_id, &secret).await.access.unwrap();
        app.login_admin().await;
        a
    };

    let rotated = app
        .post_empty(&format!("/api/v1/api-clients/{id}/rotate"))
        .await;
    assert_eq!(rotated.status, StatusCode::OK);

    // old access token now rejected
    app.logout_local();
    assert_eq!(
        app.post_json_bearer("/api/rag/query", &access, json!({ "question": "x" }))
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );

    app.login_admin().await;
    assert_eq!(
        app.delete(&format!("/api/v1/api-clients/{id}"))
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    let list = app.get("/api/v1/api-clients").await;
    assert_eq!(list.json[0]["active"], false);
}
