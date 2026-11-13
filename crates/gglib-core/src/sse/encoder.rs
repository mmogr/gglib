//! Write gglib's own `chat.completion.chunk` SSE frames.
//!
//! The proxy forwards llama-server's frames as they came; this writes only
//! the frames gglib adds to a reply: a notice as text, a repair's tool calls,
//! the finish of a turn the proxy ends itself, and an in-stream error.
//!
//! # Frame envelope
//!
//! Every chunk it writes has this shape:
//!
//! ```json
//! {
//!   "id": "chatcmpl-…",
//!   "object": "chat.completion.chunk",
//!   "created": 1729000000,
//!   "model": "qwen3-coder",
//!   "choices": [{ "index": 0, "delta": { … }, "finish_reason": null }]
//! }
//! ```
//!
//! The `id`, `model` and `created` are carried on [`SseEncoder`]; the proxy
//! gives it those of the reply its frames join.

use serde_json::{Value, json};

use crate::LlmStreamEvent;

/// The SSE stream-terminator sentinel.
///
/// Must be sent by the caller exactly once, only after the entire event
/// stream from [`SseEncoder::encode`] is truly exhausted — never bundled
/// into an individual event's encoding, since [`LlmStreamEvent::Done`] is
/// not guaranteed to be the last event (a trailing
/// [`LlmStreamEvent::Usage`] can legitimately follow it) and nothing may
/// be sent after `[DONE]` on the wire.
pub const DONE_SENTINEL: &str = "data: [DONE]\n\n";

/// Encoder for gglib's own OpenAI-shape SSE frames in one response.
///
/// The `id`, `model`, and `created` fields are the same on every frame it
/// writes.
#[derive(Debug, Clone)]
pub struct SseEncoder {
    /// Stable response id, e.g. `"chatcmpl-…"`.
    pub id: String,
    /// Model name as advertised to the client (NOT the upstream alias).
    pub model: String,
    /// Unix epoch seconds when the response was created.
    pub created: u64,
}

impl SseEncoder {
    /// Construct a new encoder with the stable response metadata.
    #[must_use]
    pub fn new(id: impl Into<String>, model: impl Into<String>, created: u64) -> Self {
        Self {
            id: id.into(),
            model: model.into(),
            created,
        }
    }

    /// Encode a text, tool-call or finish event as one SSE frame.
    ///
    /// Returns `None` for every other event: gglib writes no reasoning,
    /// progress or usage frame of its own, and an error frame is
    /// [`Self::upstream_error_frame`].
    ///
    /// For [`LlmStreamEvent::Done`], the returned `String` is only the
    /// terminating chunk (with `finish_reason` set) — it deliberately does
    /// **not** include the trailing `data: [DONE]\n\n` sentinel
    /// ([`DONE_SENTINEL`]).  `Done` is not guaranteed to be the last event on
    /// the wire: a trailing [`LlmStreamEvent::Usage`] can legitimately arrive
    /// afterward (see that variant's doc), and nothing may follow `[DONE]`
    /// once it's sent.  Callers must append [`DONE_SENTINEL`] themselves,
    /// exactly once, only after the entire event stream is truly exhausted.
    #[must_use]
    pub fn encode(&self, event: &LlmStreamEvent) -> Option<String> {
        match event {
            LlmStreamEvent::TextDelta { content } => Some(self.frame(&json!({
                "index": 0,
                "delta": { "content": content },
                "finish_reason": Value::Null,
            }))),
            LlmStreamEvent::ToolCallDelta {
                index,
                id,
                name,
                arguments,
            } => {
                let mut tc = json!({ "index": index });
                if let Some(id) = id {
                    tc["id"] = json!(id);
                    // OpenAI clients expect "type":"function" on the first
                    // delta for a given index.
                    tc["type"] = json!("function");
                }
                let mut function = json!({});
                if let Some(name) = name {
                    function["name"] = json!(name);
                }
                if let Some(arguments) = arguments {
                    function["arguments"] = json!(arguments);
                }
                if function.as_object().is_some_and(|o| !o.is_empty()) {
                    tc["function"] = function;
                }
                Some(self.frame(&json!({
                    "index": 0,
                    "delta": { "tool_calls": [tc] },
                    "finish_reason": Value::Null,
                })))
            }
            LlmStreamEvent::Done { finish_reason } => Some(self.frame(&json!({
                "index": 0,
                "delta": {},
                "finish_reason": finish_reason,
            }))),
            LlmStreamEvent::ReasoningDelta { .. }
            | LlmStreamEvent::PromptProgress { .. }
            | LlmStreamEvent::Usage { .. }
            | LlmStreamEvent::WritingTime { .. }
            | LlmStreamEvent::UpstreamError { .. } => None,
        }
    }

    /// One error frame, `data: {"error": {…}}`: the one place the in-stream
    /// error envelope is written.
    ///
    /// The proxy calls it for the failures it reports itself.
    ///
    /// Deliberately bare — no `id`/`object`/`created`/`model` envelope and,
    /// crucially, no `choices` key at all (unlike every other frame this
    /// encoder writes). Clients such as the GitHub Copilot LLM Gateway
    /// extension detect this exact shape
    /// (`'error' in obj && !('choices' in obj)`) to recognise an inline
    /// mid-stream failure; wrapping it in the usual envelope or adding an
    /// empty `choices: []` would hide it as an ordinary chunk instead.
    ///
    /// Does **not** append [`DONE_SENTINEL`] — see [`Self::encode`] doc; the
    /// caller appends it exactly once after the stream is truly exhausted.
    #[must_use]
    pub fn upstream_error_frame(message: &str, error_type: &str, code: &str) -> String {
        let error_obj = json!({
            "error": {
                "message": message,
                "type": error_type,
                "code": code,
            }
        });
        format!("data: {error_obj}\n\n")
    }

    /// Wrap a `choice` value in the standard chunk envelope and SSE framing.
    fn frame(&self, choice: &Value) -> String {
        let value = json!({
            "id": self.id,
            "object": "chat.completion.chunk",
            "created": self.created,
            "model": self.model,
            "choices": [choice],
        });
        format!("data: {value}\n\n")
    }
}

#[cfg(test)]
#[path = "encoder_tests.rs"]
mod tests;
