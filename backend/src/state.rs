use std::sync::Arc;

use crate::{
    application::{
        api_client::service::ApiClientService, auth::service::AuthService, rag::RagEngine,
        search::SearchIndex,
    },
    config::Config,
    domain::{
        chat::repository::{ChatMessageRepository, ChatThreadRepository},
        processing::repository::JobRepository,
        rag::repository::{ChunkRepository, DocumentRepository},
        rbac::repository::{PermissionRepository, RoleRepository},
    },
    infrastructure::http::ratelimit::RateLimiter,
};

pub struct AppState {
    pub config: Config,
    pub jobs: Arc<dyn JobRepository>,
    pub documents: Arc<dyn DocumentRepository>,
    pub chunks: Arc<dyn ChunkRepository>,
    pub chat_threads: Arc<dyn ChatThreadRepository>,
    pub chat_messages: Arc<dyn ChatMessageRepository>,
    pub auth: Arc<AuthService>,
    pub roles: Arc<dyn RoleRepository>,
    pub permissions: Arc<dyn PermissionRepository>,
    pub api_clients: Arc<ApiClientService>,
    /// Per-IP throttle for the authentication endpoints.
    pub auth_limiter: RateLimiter,
    /// Per-IP throttle for every other `/api` route. `None` disables it
    /// (`API_RATE_LIMIT=off`) — e.g. when a trusted proxy already limits.
    pub api_limiter: Option<RateLimiter>,
    /// The Postgres pool, when persistence is enabled. Used by the readiness
    /// probe (`GET /health/ready`); `None` in the in-memory configuration.
    pub db_pool: Option<sqlx::PgPool>,
    /// Embeddings + chat LLM + Qdrant, bundled — `/api/rag/*` needs all
    /// three, so they're gated together. `None` when `LLM_API_KEY` is unset
    /// or Qdrant couldn't be reached at startup: the routes then answer 503
    /// instead of half-working.
    pub rag: Option<RagEngine>,
    /// Pool for the external ERP/CRM database `POST /api/rag/sync/erp` pulls
    /// from — separate from `db_pool` (this service's own persistence).
    /// `None` when `ERP_DATABASE_URL` is unset or unreachable at startup:
    /// the sync route then answers 503, same convention as `rag`.
    pub erp_pool: Option<sqlx::PgPool>,
    /// Shared client for `POST /api/webhooks/dispatch` — one connection pool
    /// reused across calls rather than built per-request.
    pub webhook_client: reqwest::Client,
    /// Embedded BM25 full-text index over ingested chunks. Always present —
    /// unlike `rag`, it needs no `LLM_API_KEY`/Qdrant, just the chunk text
    /// already sitting in `chunks`.
    pub search: SearchIndex,
}
