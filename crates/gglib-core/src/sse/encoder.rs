//! Encode typed [`LlmStreamEvent`] values into OpenAI-compatible SSE
//! `chat.completion.chunk` `data:` frames.
//!
//! This is the inverse of [`super::parser::parse_sse_frame`] and is used by
//! the proxy after the universal normalization layer has rewritten model-
//! specific dialects (Qwen XML tool calls, bare `<think>` tags) into strict
//! `OpenAI` events.  Re-emitting the canonical wire format ensures external
//! clients (`OpenWebUI`, `OpenAI` SDKs, etc.) see only pristine `OpenAI` JSON
//! regardless of which model is on the other end.
//!
//! # Frame envelope
//!
//! Every emitted chunk has this shape:
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
//! Stable values (`id`, `model`, `created`) are carried on [`SseEncoder`] so
//! they are identical across every chunk of a single response.

use serde_json::{Value, json};

use crate::LlmStreamEvent;
use crate::domain::agent::ContextReading;

/// The SSE stream-terminator sentinel.
///
/// Must be sent by the caller exactly once, only after the entire event
/// stream from [`SseEncoder::encode`] is truly exhausted — never bundled
/// into an individual event's encoding, since [`LlmStreamEvent::Done`] is
/// not guaranteed to be the last event (a trailing
/// [`LlmStreamEvent::Usage`] can legitimately follow it) and nothing may
/// be sent after `[DONE]` on the wire.
pub const DONE_SENTINEL: &str = "data: [DONE]\n\n";

/// Stateful encoder that produces OpenAI-shape SSE frames for one response.
///
/// The `id`, `model`, and `created` fields are stable across all frames the
/// encoder produces, matching the `OpenAI` streaming contract.
#[derive(Debug, Clone)]
pub struct SseEncoder {
    /// Stable response id, e.g. `"chatcmpl-…"`.
    pub id: String,
    /// Model name as advertised to the client (NOT the upstream alias).
    pub model: String,
    /// Unix epoch seconds when the response was created.
    pub created: u64,
    /// What the usage frame also says of the context, for a client that
    /// asked ([`Self::with_reading`]); `None` leaves the frame as it was.
    reading: Option<ContextReading>,
}

impl SseEncoder {
    /// Construct a new encoder with the stable response metadata.
    #[must_use]
    pub fn new(id: impl Into<String>, model: impl Into<String>, created: u64) -> Self {
        Self {
            id: id.into(),
            model: model.into(),
            created,
            reading: None,
        }
    }

    /// Say `reading` inside `usage` in the usage frame, under the names a
    /// `turn_usage` event spells them by, each only when it is known.
    /// `None`, the default, writes the frame byte for byte as before.
    #[must_use]
    pub const fn with_reading(mut self, reading: Option<ContextReading>) -> Self {
        self.reading = reading;
        self
    }

    /// Encode a single [`LlmStreamEvent`] into one or more SSE frames.
    ///
    /// Returns `None` when the event is not meant to appear on the wire (e.g.
    /// [`LlmStreamEvent::NormalizationError`], which the proxy logs but never
    /// forwards to clients).
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
            LlmStreamEvent::ReasoningDelta { content } => Some(self.frame(&json!({
                "index": 0,
                "delta": { "reasoning_content": content },
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
            LlmStreamEvent::PromptProgress {
                processed,
                total,
                cached,
                time_ms,
            } => {
                // prompt_progress frames live at the top level (no `choices`).
                let value = json!({
                    "id": self.id,
                    "object": "chat.completion.chunk",
                    "created": self.created,
                    "model": self.model,
                    "prompt_progress": {
                        "processed": processed,
                        "total": total,
                        "cache": cached,
                        "time_ms": time_ms,
                    },
                });
                Some(format!("data: {value}\n\n"))
            }
            LlmStreamEvent::Done { finish_reason } => Some(self.frame(&json!({
                "index": 0,
                "delta": {},
                "finish_reason": finish_reason,
            }))),
            LlmStreamEvent::Usage {
                prompt_tokens,
                completion_tokens,
                total_tokens,
                cached_tokens,
            } => Some(self.usage_frame(
                *prompt_tokens,
                *completion_tokens,
                *total_tokens,
                *cached_tokens,
            )),
            LlmStreamEvent::NormalizationError { .. } | LlmStreamEvent::WritingTime { .. } => None,
            LlmStreamEvent::UpstreamError {
                message,
                error_type,
                code,
            } => Some(Self::upstream_error_frame(message, error_type, code)),
        }
    }

    /// Encode a [`LlmStreamEvent::Usage`] event.
    ///
    /// Per the `OpenAI` `stream_options.include_usage` convention, the
    /// usage-totals chunk carries an empty `choices` array (not omitted —
    /// see [`crate::LlmStreamEvent::Usage`] doc) and a top-level `usage`
    /// object.
    fn usage_frame(
        &self,
        prompt_tokens: u32,
        completion_tokens: u32,
        total_tokens: u32,
        cached_tokens: Option<u32>,
    ) -> String {
        let mut usage = json!({
            "prompt_tokens": prompt_tokens,
            "completion_tokens": completion_tokens,
            "total_tokens": total_tokens,
        });
        // Re-emitted only when the upstream reported it, so the frame stays
        // byte-identical to before for servers that don't. Clients such as the
        // Copilot LLM Gateway extension surface this as `promptTokenDetails`.
        if let Some(cached) = cached_tokens {
            usage["prompt_tokens_details"] = json!({ "cached_tokens": cached });
        }
        // A reading is merged only for a client that asked for one. Its own
        // serialisation leaves out what is unknown, so no key is `null`.
        if let Some(reading) = self.reading
            && let Ok(Value::Object(known)) = serde_json::to_value(reading)
            && let Some(usage) = usage.as_object_mut()
        {
            usage.extend(known);
        }
        let value = json!({
            "id": self.id,
            "object": "chat.completion.chunk",
            "created": self.created,
            "model": self.model,
            "choices": [],
            "usage": usage,
        });
        format!("data: {value}\n\n")
    }

    /// One error frame, `data: {"error": {…}}`: the one place the in-stream
    /// error envelope is written.
    ///
    /// An [`LlmStreamEvent::UpstreamError`] is encoded with it, and the proxy
    /// calls it for the failures it reports itself.
    ///
    /// Deliberately bare — no `id`/`object`/`created`/`model` envelope and,
    /// crucially, no `choices` key at all (unlike every other frame this
    /// encoder produces). Clients such as the GitHub Copilot LLM Gateway
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

#[cfg(test)]
#[path = "encoder_reading_tests.rs"]
mod reading_tests;
