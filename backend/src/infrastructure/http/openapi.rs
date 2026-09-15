//! OpenAPI document for the public RAG API.
//!
//! Only the endpoints an external integrator calls are described here — the
//! session-cookie admin routes under `/api/v1/*` are intentionally excluded.
//!
//! Served (spec + Scalar UI at `/api/docs` + Swagger UI at `/api/swagger`) only
//! when [`crate::config::Config::enable_api_docs`]
//! is true; see [`super::router`].

use utoipa::{
    Modify, OpenApi,
    openapi::security::{ApiKey, ApiKeyValue, HttpAuthScheme, HttpBuilder, SecurityScheme},
};

/// `multipart/form-data` body: a single PDF to ingest.
#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
pub struct UploadForm {
    #[schema(value_type = String, format = Binary)]
    pub file: String,
}

/// JSON body for `POST /api/rag/query`.
#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
pub struct RagQueryBody {
    pub question: String,
    /// How many chunks to retrieve. Defaults to `RAG_TOP_K`.
    pub top_k: Option<u64>,
    /// Narrow retrieval to just these documents. Omit to search everything
    /// this caller has ingested.
    pub document_ids: Option<Vec<uuid::Uuid>>,
}

/// JSON body for the external OAuth2 token endpoint.
#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
pub struct TokenBody {
    pub client_id: String,
    pub client_secret: String,
}

/// JSON body for the refresh endpoint.
#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
pub struct RefreshBody {
    pub refresh_token: String,
}

struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "api_key",
            SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::with_description(
                "x-api-key",
                "Static service API key.",
            ))),
        );
        components.add_security_scheme(
            "bearer",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .description(Some(
                        "OAuth2 client-credentials access token from `POST /api/ext/auth/token`.",
                    ))
                    .build(),
            ),
        );
    }
}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "RAG API",
        version = "0.1.0",
        description = "Ingest PDFs into a vector index (Qdrant) and ask \
                       questions answered strictly from what's indexed, with \
                       inline citations. Authenticate with a static \
                       `x-api-key` header or an OAuth2 bearer token. \
                       `rag:ingest` covers uploading/listing/deleting \
                       documents; `rag:query` covers asking questions. Both \
                       routes answer 503 unless the server has `LLM_API_KEY` \
                       configured and Qdrant is reachable.",
    ),
    servers((url = "/", description = "This host")),
    paths(
        crate::infrastructure::http::handlers::rag::ingest,
        crate::infrastructure::http::handlers::rag::list,
        crate::infrastructure::http::handlers::rag::detail,
        crate::infrastructure::http::handlers::rag::delete,
        crate::infrastructure::http::handlers::rag::query,
        crate::infrastructure::http::handlers::ext_auth::token,
        crate::infrastructure::http::handlers::ext_auth::refresh,
    ),
    components(schemas(
        UploadForm, RagQueryBody, TokenBody, RefreshBody,
        crate::infrastructure::http::handlers::rag::QueryResult,
        crate::infrastructure::http::handlers::rag::QuerySource,
    )),
    tags(
        (name = "rag", description = "Ingest documents and ask grounded questions over them"),
        (name = "auth", description = "OAuth2 client-credentials for external clients"),
    ),
    modifiers(&SecurityAddon),
)]
pub struct ApiDoc;
