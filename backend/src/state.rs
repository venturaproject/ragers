use std::sync::Arc;

use crate::{
    application::{
        api_client::service::ApiClientService, auth::service::AuthService, rag::RagEngine,
    },
    config::Config,
    domain::{
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
}
