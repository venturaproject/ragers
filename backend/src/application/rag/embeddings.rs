//! NVIDIA NIM embeddings client — `/embeddings`, the OpenAI-compatible shape
//! plus NIM's `input_type`. NIM's retrieval models are *asymmetric*: a
//! passage (indexed chunk) and a query (question) are embedded differently,
//! and mixing them up quietly wrecks retrieval quality — so `InputType` is
//! required at every call site, not an afterthought default.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::errors::{AppError, AppResult};

#[derive(Debug, Clone, Copy)]
pub enum InputType {
    Query,
    Passage,
}

impl InputType {
    fn as_str(self) -> &'static str {
        match self {
            InputType::Query => "query",
            InputType::Passage => "passage",
        }
    }
}

#[derive(Clone)]
pub struct EmbeddingsClient {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
    model: String,
}

impl EmbeddingsClient {
    pub fn new(base_url: &str, api_key: &str, model: &str, timeout_secs: u64) -> AppResult<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .user_agent("rag-api-embeddings/1")
            .build()
            .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
        Ok(Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key: api_key.to_string(),
            model: model.to_string(),
        })
    }

    /// Embed a batch of texts in one request. The returned vectors match
    /// `inputs` 1:1 in order (reordered from the upstream response's
    /// `index`, which providers aren't guaranteed to return in request
    /// order); the token usage, when the upstream reports one, is what gets
    /// billed against an API client's AI-token quota for an ingest call.
    pub async fn embed(
        &self,
        inputs: &[String],
        kind: InputType,
    ) -> AppResult<(Vec<Vec<f32>>, Option<u32>)> {
        if inputs.is_empty() {
            return Ok((Vec::new(), None));
        }

        let body = Request {
            model: &self.model,
            input: inputs,
            input_type: kind.as_str(),
            encoding_format: "float",
        };

        let resp = self
            .client
            .post(format!("{}/embeddings", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("embeddings request failed: {e}")))?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            let snippet: String = text.chars().take(300).collect();
            return Err(AppError::BadRequest(format!(
                "embeddings upstream returned {status}: {snippet}"
            )));
        }

        let parsed: Response = resp.json().await.map_err(|e| {
            AppError::Internal(anyhow::anyhow!(
                "embeddings upstream returned an unexpected body: {e}"
            ))
        })?;

        let mut out = vec![Vec::new(); inputs.len()];
        for item in parsed.data {
            if let Some(slot) = out.get_mut(item.index) {
                *slot = item.embedding;
            }
        }
        if out.iter().any(Vec::is_empty) {
            return Err(AppError::Internal(anyhow::anyhow!(
                "embeddings upstream returned fewer vectors than inputs"
            )));
        }
        Ok((out, parsed.usage.map(|u| u.total_tokens)))
    }
}

#[derive(Serialize)]
struct Request<'a> {
    model: &'a str,
    input: &'a [String],
    input_type: &'a str,
    encoding_format: &'a str,
}

#[derive(Deserialize)]
struct Response {
    data: Vec<Item>,
    usage: Option<Usage>,
}

#[derive(Deserialize)]
struct Item {
    embedding: Vec<f32>,
    index: usize,
}

#[derive(Deserialize)]
struct Usage {
    total_tokens: u32,
}
