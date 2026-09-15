//! Wraps `rag::service::answer_question` with persistence: every question
//! asked in a thread is saved as a `user` message, the answer (+ sources) as
//! an `assistant` message, and the thread's `updated_at` is bumped so it
//! sorts to the top of the list. The RAG call itself is untouched — no
//! thread history is fed back into the prompt, so answers stay grounded
//! strictly in what's indexed rather than drifting with conversation context.

use std::sync::Arc;
use uuid::Uuid;

use crate::{
    application::rag::{self, RagEngine, service::AnswerOutcome},
    domain::chat::{
        entities::{ChatMessage, MessageRole},
        repository::{ChatMessageRepository, ChatThreadRepository},
    },
    errors::AppResult,
};

fn sources_to_json(outcome: &AnswerOutcome) -> serde_json::Value {
    serde_json::json!(
        outcome
            .sources
            .iter()
            .map(|s| serde_json::json!({
                "document_id": s.document_id,
                "chunk_index": s.chunk_index,
                "text": s.text,
                "score": s.score,
            }))
            .collect::<Vec<_>>()
    )
}

/// Save the user's question, ask the RAG engine, save the answer (or the
/// error, so a failed attempt still shows up in the thread), and return the
/// outcome for the caller to build its own response from.
#[allow(clippy::too_many_arguments)]
pub async fn ask_in_thread(
    engine: &RagEngine,
    threads: &Arc<dyn ChatThreadRepository>,
    messages: &Arc<dyn ChatMessageRepository>,
    thread_id: Uuid,
    owner: Option<&str>,
    document_ids: Option<&[Uuid]>,
    question: &str,
    top_k: Option<u64>,
) -> AppResult<AnswerOutcome> {
    messages
        .create(ChatMessage::new(
            thread_id,
            MessageRole::User,
            question.to_string(),
            None,
        ))
        .await?;

    let result = rag::service::answer_question(engine, owner, document_ids, question, top_k).await;

    let assistant_content = match &result {
        Ok(outcome) => outcome.answer.clone(),
        Err(e) => e.to_string(),
    };
    let assistant_sources = result.as_ref().ok().map(sources_to_json);
    messages
        .create(ChatMessage::new(
            thread_id,
            MessageRole::Assistant,
            assistant_content,
            assistant_sources,
        ))
        .await?;
    threads.touch(thread_id).await?;

    result
}
