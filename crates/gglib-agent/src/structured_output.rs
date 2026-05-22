//! Structured-output helper for the orchestrator director.
//!
//! [`get_structured`] wraps [`LlmCompletionPort::chat_stream`] with a
//! JSON Schema constraint and retry loop so the caller gets a fully
//! typed, validated response or a [`StructuredOutputError`] that
//! explains exactly what went wrong.
//!
//! # Retry strategy
//!
//! On a JSON parse failure the function appends two messages to the
//! conversation history and retries:
//!
//! 1. An `assistant` message containing the raw (unparseable) output.
//! 2. A `user` message explaining the parse error and asking for a
//!    corrected response.
//!
//! This gives the model its own broken output as context, which
//! typically leads to rapid self-correction.
//!
//! # Example
//!
//! ```rust,ignore
//! use serde::Deserialize;
//! use gglib_agent::structured_output::get_structured;
//! use gglib_core::{AgentMessage, ResponseFormat};
//!
//! #[derive(Deserialize)]
//! struct Plan { steps: Vec<String> }
//!
//! let schema = serde_json::json!({
//!     "type": "object",
//!     "properties": { "steps": { "type": "array", "items": { "type": "string" } } },
//!     "required": ["steps"]
//! });
//! let messages = vec![
//!     AgentMessage::System { content: "You are a planner.".into() },
//!     AgentMessage::User { content: "List three steps.".into() },
//! ];
//! let plan: Plan = get_structured(&llm, messages, schema, 2).await?;
//! ```

use std::sync::Arc;

use futures_util::StreamExt as _;
use serde::de::DeserializeOwned;

use gglib_core::{
    AgentMessage, LlmStreamEvent, ResponseFormat, StructuredOutputError, ports::LlmCompletionPort,
};

/// Call the LLM with a JSON Schema constraint and deserialise the result as `T`.
///
/// # Parameters
///
/// - `llm` — the LLM port to call.
/// - `messages` — the conversation history / prompt.
/// - `schema` — a JSON Schema object that the model's output must conform to.
/// - `max_retries` — how many additional attempts to make after the first
///   failure.  `0` means try once with no retries.
///
/// # Returns
///
/// `Ok(T)` when the model produces valid JSON that deserialises to `T`, or a
/// [`StructuredOutputError`] describing the failure.
///
/// # Errors
///
/// - [`StructuredOutputError::Stream`] — the LLM stream itself failed.
/// - [`StructuredOutputError::Parse`] — the output was not valid JSON / `T`.
/// - [`StructuredOutputError::MaxRetriesExceeded`] — all attempts failed.
pub async fn get_structured<T: DeserializeOwned>(
    llm: &Arc<dyn LlmCompletionPort>,
    messages: Vec<AgentMessage>,
    schema: serde_json::Value,
    max_retries: u32,
) -> Result<T, StructuredOutputError> {
    let format = ResponseFormat::JsonSchema {
        schema,
        strict: false,
    };
    let mut history = messages;
    let mut last_error = String::new();

    for attempt in 0..=max_retries {
        // Call the LLM.
        let stream = llm
            .chat_stream(&history, &[], Some(&format))
            .await
            .map_err(StructuredOutputError::Stream)?;

        // Collect all text deltas.
        let raw = collect_text(stream)
            .await
            .map_err(StructuredOutputError::Stream)?;

        // Try to parse.
        match serde_json::from_str::<T>(&raw) {
            Ok(value) => return Ok(value),
            Err(e) => {
                last_error = e.to_string();

                if attempt < max_retries {
                    // Feed the broken output back so the model can self-correct.
                    history.push(AgentMessage::Assistant {
                        content: crate::structured_output::AssistantRaw(raw.clone()).into(),
                    });
                    history.push(AgentMessage::User {
                        content: format!(
                            "Your previous response was not valid JSON: {last_error}\n\
                             Raw output:\n{raw}\n\n\
                             Please respond with a single valid JSON object that matches the schema."
                        ),
                    });
                } else {
                    return Err(StructuredOutputError::Parse {
                        error: last_error,
                        raw,
                        attempts: attempt + 1,
                    });
                }
            }
        }
    }

    Err(StructuredOutputError::MaxRetriesExceeded {
        max_retries,
        last_error,
    })
}

/// Collect only [`LlmStreamEvent::TextDelta`] events from a stream, discarding
/// reasoning deltas, tool-call deltas, and other event kinds.
async fn collect_text(
    stream: std::pin::Pin<
        Box<dyn futures_core::Stream<Item = anyhow::Result<LlmStreamEvent>> + Send>,
    >,
) -> anyhow::Result<String> {
    let mut text = String::new();
    let mut stream = std::pin::pin!(stream);
    while let Some(event) = stream.next().await {
        match event? {
            LlmStreamEvent::TextDelta { content } => text.push_str(&content),
            LlmStreamEvent::Done { .. } => break,
            _ => {}
        }
    }
    Ok(text)
}

// ---------------------------------------------------------------------------
// Internal helper: wrap a raw string into an `AssistantContent` so we can
// push the model's broken output back into the conversation history without
// exposing a new public type.
// ---------------------------------------------------------------------------

/// Newtype wrapper used internally to convert a raw JSON string into the
/// [`AgentMessage::Assistant`] `content` field.
struct AssistantRaw(String);

impl From<AssistantRaw> for gglib_core::AssistantContent {
    fn from(raw: AssistantRaw) -> Self {
        gglib_core::AssistantContent {
            text: Some(raw.0),
            tool_calls: vec![],
        }
    }
}
