//! Grounded answer generation — ask the chat model to answer strictly from
//! the retrieved chunks, citing them, rather than free-associating.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::errors::{AppError, AppResult};

#[derive(Debug, Clone, serde::Serialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// A retrieved chunk, as context for the prompt.
pub struct Source<'a> {
    pub label: &'a str,
    pub text: &'a str,
}

#[derive(Clone)]
pub struct AnswerClient {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
    model: String,
    max_tokens: u32,
}

impl AnswerClient {
    pub fn new(
        base_url: &str,
        api_key: &str,
        model: &str,
        max_tokens: u32,
        timeout_secs: u64,
    ) -> AppResult<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .user_agent("rag-api-answer/1")
            .build()
            .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
        Ok(Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key: api_key.to_string(),
            model: model.to_string(),
            max_tokens,
        })
    }

    /// Answer `question` using only `sources` — the prompt explicitly forbids
    /// answering from anything else, and asks for `[n]`-style inline
    /// citations matching `sources`' order (1-based).
    pub async fn answer(
        &self,
        question: &str,
        sources: &[Source<'_>],
    ) -> AppResult<(String, Option<Usage>)> {
        let context = sources
            .iter()
            .enumerate()
            .map(|(i, s)| format!("[{}] ({})\n{}", i + 1, s.label, s.text))
            .collect::<Vec<_>>()
            .join("\n\n");

        let prompt = format!(
            "Answer the question using ONLY the numbered sources below. Cite \
             the sources you used inline as [1], [2], etc. If the sources do \
             not contain the answer, say so plainly rather than guessing.\n\n\
             Sources:\n{context}\n\nQuestion: {question}"
        );

        let body = ChatRequest {
            model: &self.model,
            messages: &[ChatMessage {
                role: "user",
                content: &prompt,
            }],
            max_tokens: self.max_tokens,
            temperature: 0.0,
            stream: false,
        };

        let resp = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("answer request failed: {e}")))?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            let snippet: String = text.chars().take(300).collect();
            return Err(AppError::BadRequest(format!(
                "answer upstream returned {status}: {snippet}"
            )));
        }

        let parsed: ChatResponse = resp.json().await.map_err(|e| {
            AppError::Internal(anyhow::anyhow!(
                "answer upstream returned an unexpected body: {e}"
            ))
        })?;

        let choice = parsed.choices.into_iter().next().ok_or_else(|| {
            AppError::Internal(anyhow::anyhow!("answer upstream returned no choices"))
        })?;

        Ok((
            choice
                .message
                .content
                .unwrap_or_default()
                .trim()
                .to_string(),
            parsed.usage.map(|u| Usage {
                prompt_tokens: u.prompt_tokens,
                completion_tokens: u.completion_tokens,
                total_tokens: u.total_tokens,
            }),
        ))
    }
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: &'a [ChatMessage<'a>],
    max_tokens: u32,
    temperature: f32,
    stream: bool,
}

#[derive(Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
    usage: Option<UpstreamUsage>,
}

#[derive(Deserialize)]
struct Choice {
    message: ResponseMessage,
}

#[derive(Deserialize)]
struct ResponseMessage {
    content: Option<String>,
}

#[derive(Deserialize)]
struct UpstreamUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}
