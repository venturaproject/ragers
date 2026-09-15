use std::env;

#[derive(Clone, Debug)]
pub struct Config {
    pub port: u16,
    pub api_keys: Vec<String>,
    pub max_file_size_mb: usize,
    pub cors_origins: Vec<String>,
    /// Public app name served at `GET /api/v1/config`.
    pub app_name: String,
    /// Add `Secure` to the session cookie (enable behind HTTPS).
    pub session_cookie_secure: bool,
    /// Admin account seeded into the in-memory user store at startup.
    pub seed_user: SeedUser,
    /// Default per-client rate limit (`"<n>/<seconds>"`) when a client sets none.
    pub ext_default_rate_limit: Option<String>,
    /// Default monthly page quota for external clients that set none.
    /// (Kept for the RBAC/API-client scaffold's sake; nothing in this
    /// service counts "pages" — `ext_default_ai_token_quota` is the one that
    /// actually gates `/api/rag/*`.)
    pub ext_default_monthly_page_quota: Option<u64>,
    /// Default monthly AI-token quota (spent on `/api/rag/documents` and
    /// `/api/rag/query`) for external clients that set none.
    pub ext_default_ai_token_quota: Option<u64>,
    /// Trust `X-Forwarded-For` / `X-Real-IP` (true when behind the nginx proxy).
    pub trust_proxy: bool,
    /// Per-IP budget for the auth endpoints: `(max_attempts, window_seconds)`.
    pub auth_rate_limit: (u32, u64),
    /// Per-IP budget for every other `/api` route. `None` = disabled
    /// (`API_RATE_LIMIT=off`).
    pub api_rate_limit: Option<(u32, u64)>,
    /// Seed the two demo `user` accounts (`maria@` / `carlos@`, password
    /// `demo1234`). Off by default — never enable in production.
    pub seed_demo_users: bool,
    /// `APP_ENV` is `production` / `prod`. Turns [`Config::validate`] from
    /// warnings into hard startup errors.
    pub production: bool,
    /// Postgres connection string. When set, users / sessions / API clients /
    /// client tokens / RAG documents are persisted there; otherwise they
    /// live in memory.
    pub database_url: Option<String>,
    /// Redis connection string (`REDIS_URL`). When set, the per-IP rate limiters
    /// are shared across replicas via Redis; otherwise they are process-local.
    pub redis_url: Option<String>,
    /// Serve the OpenAPI spec (`/api/openapi.json`) plus the Scalar
    /// (`/api/docs`) and Swagger UI (`/api/swagger`) viewers. `ENABLE_API_DOCS`
    /// overrides; unset defaults to "on outside production, off in production".
    pub enable_api_docs: bool,

    // ── RAG (`/api/rag/*`) ───────────────────────────────────────────────
    /// Qdrant gRPC endpoint. Always has a default (the bundled service) — a
    /// missing `LLM_API_KEY` is what actually disables the routes; an
    /// unreachable Qdrant fails startup instead, since it's core to this
    /// service rather than an optional integration.
    pub qdrant_url: String,
    pub qdrant_collection: String,
    /// Drives both the embeddings and the answer-generation calls — one
    /// NVIDIA NIM (or other OpenAI-compatible host) account behind both.
    /// The upstream key never reaches a caller — they authenticate against
    /// *this* API, same as every other endpoint. Absent ⇒ `/api/rag/*`
    /// answers 503, it does not half-configure.
    pub llm_api_key: Option<String>,
    pub llm_base_url: String,
    pub embeddings_model: String,
    pub chat_model: String,
    /// Ceiling applied to the answer call's `max_tokens` — a cost guard
    /// against a runaway request.
    pub llm_max_tokens: u32,
    pub llm_timeout_secs: u64,
    /// Chunking window for ingested document text, in characters.
    pub chunk_size_chars: usize,
    pub chunk_overlap_chars: usize,
    /// Default `top_k` for `/api/rag/query` when a caller doesn't set one.
    pub rag_top_k: u64,
    /// Connection string for an external ERP/CRM-style Postgres database that
    /// `POST /api/rag/sync/erp` reads products/orders from and ingests into
    /// the RAG index — a separate system from [`Config::database_url`] (this
    /// service's own persistence), simulating pulling knowledge from a
    /// system the API doesn't own. `None` ⇒ the sync route answers 503.
    pub erp_database_url: Option<String>,
}

#[derive(Clone, Debug)]
pub struct SeedUser {
    pub email: String,
    pub password: String,
    pub name: String,
    pub api_key: String,
}

impl Config {
    pub fn from_env() -> Self {
        let production = env::var("APP_ENV")
            .map(|v| matches!(v.trim().to_lowercase().as_str(), "production" | "prod"))
            .unwrap_or(false);

        Self {
            port: env::var("PORT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(8000),
            api_keys: env::var("API_KEYS")
                .unwrap_or_else(|_| "dev-key".into())
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            max_file_size_mb: env::var("MAX_FILE_SIZE_MB")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(100),
            cors_origins: env::var("CORS_ALLOWED_ORIGINS")
                .unwrap_or_else(|_| "*".into())
                .split(',')
                .map(|s| s.trim().to_string())
                .collect(),
            app_name: env::var("APP_NAME").unwrap_or_else(|_| "RAG API".into()),
            session_cookie_secure: env::var("SESSION_COOKIE_SECURE")
                .map(|v| matches!(v.trim().to_lowercase().as_str(), "1" | "true" | "yes"))
                .unwrap_or(false),
            seed_user: SeedUser {
                email: env::var("SEED_USER_EMAIL")
                    .unwrap_or_else(|_| "admin@rag.test".into())
                    .trim()
                    .to_lowercase(),
                password: env::var("SEED_USER_PASSWORD").unwrap_or_else(|_| "admin1234".into()),
                name: env::var("SEED_USER_NAME").unwrap_or_else(|_| "Admin".into()),
                api_key: env::var("SEED_USER_API_KEY")
                    .ok()
                    .or_else(|| {
                        env::var("API_KEYS").ok().and_then(|k| {
                            k.split(',')
                                .map(|s| s.trim().to_string())
                                .find(|s| !s.is_empty())
                        })
                    })
                    .unwrap_or_else(|| "dev-key".into()),
            },
            ext_default_rate_limit: env::var("EXT_DEFAULT_RATE_LIMIT")
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|s| !s.is_empty()),
            ext_default_monthly_page_quota: env::var("EXT_DEFAULT_MONTHLY_PAGE_QUOTA")
                .ok()
                .and_then(|v| v.trim().parse().ok())
                .filter(|q| *q > 0),
            ext_default_ai_token_quota: env::var("EXT_DEFAULT_AI_TOKEN_QUOTA")
                .ok()
                .and_then(|v| v.trim().parse().ok())
                .filter(|q| *q > 0),
            trust_proxy: env::var("TRUST_PROXY")
                .map(|v| !matches!(v.trim().to_lowercase().as_str(), "0" | "false" | "no"))
                .unwrap_or(true),
            auth_rate_limit: env::var("AUTH_RATE_LIMIT")
                .ok()
                .and_then(|v| {
                    let (n, w) = v.trim().split_once('/')?;
                    Some((n.trim().parse().ok()?, w.trim().parse().ok()?))
                })
                .filter(|(n, w): &(u32, u64)| *n > 0 && *w > 0)
                .unwrap_or((10, 60)),
            api_rate_limit: match env::var("API_RATE_LIMIT").ok().as_deref().map(str::trim) {
                Some("off") | Some("0") => None,
                Some(v) => v
                    .split_once('/')
                    .and_then(|(n, w)| Some((n.trim().parse().ok()?, w.trim().parse().ok()?)))
                    .filter(|(n, w): &(u32, u64)| *n > 0 && *w > 0)
                    .or(Some((120, 60))),
                None => Some((120, 60)),
            },
            seed_demo_users: env::var("SEED_DEMO_USERS")
                .map(|v| matches!(v.trim().to_lowercase().as_str(), "1" | "true" | "yes"))
                .unwrap_or(false),
            production,
            database_url: env::var("DATABASE_URL")
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|s| !s.is_empty()),
            redis_url: env::var("REDIS_URL")
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|s| !s.is_empty()),
            enable_api_docs: match env::var("ENABLE_API_DOCS").ok().as_deref().map(str::trim) {
                Some(v) if !v.is_empty() => {
                    matches!(v.to_lowercase().as_str(), "1" | "true" | "yes" | "on")
                }
                _ => !production,
            },
            qdrant_url: env::var("QDRANT_URL").unwrap_or_else(|_| "http://qdrant:6334".into()),
            qdrant_collection: env::var("QDRANT_COLLECTION").unwrap_or_else(|_| "documents".into()),
            llm_api_key: env::var("LLM_API_KEY")
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|s| !s.is_empty()),
            llm_base_url: env::var("LLM_BASE_URL")
                .unwrap_or_else(|_| "https://integrate.api.nvidia.com/v1".into()),
            embeddings_model: env::var("EMBEDDINGS_MODEL")
                .unwrap_or_else(|_| "nvidia/nemotron-3-embed-1b".into()),
            chat_model: env::var("CHAT_MODEL")
                .unwrap_or_else(|_| "meta/llama-3.2-11b-vision-instruct".into()),
            llm_max_tokens: env::var("LLM_MAX_TOKENS")
                .ok()
                .and_then(|v| v.trim().parse().ok())
                .filter(|n| *n > 0)
                .unwrap_or(1024),
            llm_timeout_secs: env::var("LLM_TIMEOUT_SECS")
                .ok()
                .and_then(|v| v.trim().parse().ok())
                .filter(|n| *n > 0)
                .unwrap_or(60),
            chunk_size_chars: env::var("CHUNK_SIZE_CHARS")
                .ok()
                .and_then(|v| v.trim().parse().ok())
                .filter(|n| *n > 0)
                .unwrap_or(1200),
            chunk_overlap_chars: env::var("CHUNK_OVERLAP_CHARS")
                .ok()
                .and_then(|v| v.trim().parse().ok())
                .unwrap_or(150),
            rag_top_k: env::var("RAG_TOP_K")
                .ok()
                .and_then(|v| v.trim().parse().ok())
                .filter(|n| *n > 0)
                .unwrap_or(8),
            erp_database_url: env::var("ERP_DATABASE_URL")
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|s| !s.is_empty()),
        }
    }

    /// Startup sanity checks. Returns `(errors, warnings)`.
    ///
    /// In production (`APP_ENV=production`) the errors are fatal — `main` refuses
    /// to boot. Outside production everything is a warning so local dev with the
    /// bundled defaults still works.
    pub fn validate(&self) -> (Vec<String>, Vec<String>) {
        const WEAK_PASSWORDS: &[&str] = &[
            "admin1234",
            "password",
            "changeme",
            "change-me",
            "secret",
            "demo1234",
        ];
        const WEAK_KEYS: &[&str] = &["dev-key", "change-me-in-production", "changeme", "test-key"];

        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        let pw = self.seed_user.password.as_str();
        if WEAK_PASSWORDS.contains(&pw) || pw.len() < 12 {
            errors.push(format!(
                "SEED_USER_PASSWORD is weak or a known default ({} chars) — set a strong value",
                pw.len()
            ));
        }
        if self.api_keys.is_empty() {
            warnings.push("API_KEYS is empty — service-key access is disabled".into());
        }
        if self
            .api_keys
            .iter()
            .any(|k| WEAK_KEYS.contains(&k.as_str()))
        {
            errors.push("API_KEYS contains a known default key — rotate it".into());
        }
        if self.seed_demo_users {
            errors.push(
                "SEED_DEMO_USERS is enabled — the maria@/carlos@ accounts use a public password"
                    .into(),
            );
        }
        if self.cors_origins.iter().any(|o| o == "*") {
            warnings.push(
                "CORS_ALLOWED_ORIGINS is '*' — set explicit origins for a browser-facing deployment"
                    .into(),
            );
        }
        if !self.session_cookie_secure {
            warnings.push(
                "SESSION_COOKIE_SECURE is false — the session cookie has no `Secure` flag".into(),
            );
        }
        if self.database_url.is_none() {
            errors.push(
                "DATABASE_URL is not set — auth state would be in-memory and lost on restart"
                    .into(),
            );
        }
        if self.enable_api_docs {
            warnings.push(
                "ENABLE_API_DOCS is on — the OpenAPI spec + Scalar/Swagger UIs are publicly reachable at /api/{docs,swagger}"
                    .into(),
            );
        }
        if self.production {
            (errors, warnings)
        } else {
            // Dev: nothing is fatal.
            warnings.append(&mut errors);
            (Vec::new(), warnings)
        }
    }

    /// Validate an incoming API key in constant time.
    ///
    /// Every configured key is compared regardless of an early match so the
    /// response time does not leak which prefix (or how many characters) of a
    /// guess was correct.
    pub fn is_valid_key(&self, key: &str) -> bool {
        let mut valid = false;
        for k in &self.api_keys {
            valid |= constant_time_eq(k.as_bytes(), key.as_bytes());
        }
        valid
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::field_reassign_with_default)]
mod tests {
    use super::*;

    fn cfg() -> Config {
        Config {
            port: 0,
            api_keys: vec!["k1".into(), "k2".into()],
            max_file_size_mb: 10,
            cors_origins: vec!["*".into()],
            app_name: "T".into(),
            session_cookie_secure: false,
            seed_user: SeedUser {
                email: "a@b.c".into(),
                password: "pw".into(),
                name: "A".into(),
                api_key: "k1".into(),
            },
            ext_default_rate_limit: None,
            ext_default_monthly_page_quota: None,
            ext_default_ai_token_quota: None,
            trust_proxy: false,
            auth_rate_limit: (10, 60),
            api_rate_limit: Some((120, 60)),
            seed_demo_users: false,
            production: false,
            database_url: None,
            redis_url: None,
            enable_api_docs: false,
            qdrant_url: "http://localhost:6334".into(),
            qdrant_collection: "documents".into(),
            llm_api_key: None,
            llm_base_url: "https://integrate.api.nvidia.com/v1".into(),
            embeddings_model: "nvidia/nemotron-3-embed-1b".into(),
            chat_model: "meta/llama-3.2-11b-vision-instruct".into(),
            llm_max_tokens: 1024,
            llm_timeout_secs: 60,
            chunk_size_chars: 1200,
            chunk_overlap_chars: 150,
            rag_top_k: 5,
            erp_database_url: None,
        }
    }

    #[test]
    fn validate_is_advisory_in_dev_but_fatal_in_prod() {
        // Bundled dev defaults: warnings only, never fatal.
        let mut c = cfg();
        c.api_keys = vec!["dev-key".into()];
        c.seed_user.password = "admin1234".into();
        let (errors, warnings) = c.validate();
        assert!(errors.is_empty());
        assert!(!warnings.is_empty());

        // Same config in production: the weak password + default key are fatal.
        c.production = true;
        let (errors, _) = c.validate();
        assert!(errors.iter().any(|e| e.contains("SEED_USER_PASSWORD")));
        assert!(errors.iter().any(|e| e.contains("API_KEYS")));

        // A hardened production config passes.
        c.api_keys = vec!["S3rvïce-Key-9f2a8c1d4e6b".into()];
        c.seed_user.password = "a-long-strong-passphrase".into();
        c.cors_origins = vec!["https://app.example.com".into()];
        c.session_cookie_secure = true;
        c.seed_demo_users = false;
        c.database_url = Some("postgres://localhost/rag".into());
        let (errors, warnings) = c.validate();
        assert!(errors.is_empty(), "{errors:?}");
        assert!(warnings.is_empty(), "{warnings:?}");
    }

    #[test]
    fn valid_key_check_is_exact() {
        let c = cfg();
        assert!(c.is_valid_key("k1"));
        assert!(c.is_valid_key("k2"));
        assert!(!c.is_valid_key("k1 "));
        assert!(!c.is_valid_key("k"));
        assert!(!c.is_valid_key(""));
    }
}
