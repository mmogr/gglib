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
            LlmStreamEvent::NormalizationError { .. } => None,
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

    /// Encode a [`LlmStreamEvent::UpstreamError`] event.
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
    fn upstream_error_frame(message: &str, error_type: &str, code: &str) -> String {
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

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::normalize::NormalizationErrorKind;

    fn enc() -> SseEncoder {
        SseEncoder::new("chatcmpl-1", "test-model", 1_729_000_000)
    }

    fn parse_data_frame(out: &str) -> serde_json::Value {
        let line = out.lines().next().expect("non-empty output");
        let payload = line.strip_prefix("data: ").expect("data: prefix");
        serde_json::from_str(payload).expect("valid JSON")
    }

    #[test]
    fn text_delta_encodes_to_content_chunk() {
        let out = enc()
            .encode(&LlmStreamEvent::TextDelta {
                content: "hello".to_owned(),
            })
            .expect("frame");
        assert!(out.starts_with("data: "));
        assert!(out.ends_with("\n\n"));
        let v = parse_data_frame(&out);
        assert_eq!(v["object"], "chat.completion.chunk");
        assert_eq!(v["id"], "chatcmpl-1");
        assert_eq!(v["model"], "test-model");
        assert_eq!(v["choices"][0]["delta"]["content"], "hello");
        assert!(v["choices"][0]["finish_reason"].is_null());
    }

    #[test]
    fn reasoning_delta_encodes_to_reasoning_content_chunk() {
        let out = enc()
            .encode(&LlmStreamEvent::ReasoningDelta {
                content: "think".to_owned(),
            })
            .expect("frame");
        let v = parse_data_frame(&out);
        assert_eq!(v["choices"][0]["delta"]["reasoning_content"], "think");
    }

    #[test]
    fn tool_call_delta_first_frame_includes_id_and_type() {
        let out = enc()
            .encode(&LlmStreamEvent::ToolCallDelta {
                index: 0,
                id: Some("tc1".to_owned()),
                name: Some("search".to_owned()),
                arguments: Some(r#"{"q":"r"}"#.to_owned()),
            })
            .expect("frame");
        let v = parse_data_frame(&out);
        let tc = &v["choices"][0]["delta"]["tool_calls"][0];
        assert_eq!(tc["index"], 0);
        assert_eq!(tc["id"], "tc1");
        assert_eq!(tc["type"], "function");
        assert_eq!(tc["function"]["name"], "search");
        assert_eq!(tc["function"]["arguments"], r#"{"q":"r"}"#);
    }

    #[test]
    fn tool_call_delta_continuation_omits_id_and_type() {
        let out = enc()
            .encode(&LlmStreamEvent::ToolCallDelta {
                index: 0,
                id: None,
                name: None,
                arguments: Some("more".to_owned()),
            })
            .expect("frame");
        let v = parse_data_frame(&out);
        let tc = &v["choices"][0]["delta"]["tool_calls"][0];
        assert!(tc.get("id").is_none(), "id must be omitted on continuation");
        assert!(
            tc.get("type").is_none(),
            "type must be omitted on continuation"
        );
        assert_eq!(tc["function"]["arguments"], "more");
    }

    #[test]
    fn done_event_emits_only_finish_chunk_no_sentinel() {
        let out = enc()
            .encode(&LlmStreamEvent::Done {
                finish_reason: "stop".to_owned(),
            })
            .expect("frame");
        // Exactly one SSE frame -- [DONE] is the caller's responsibility now
        // (see DONE_SENTINEL doc), since a trailing Usage event can
        // legitimately follow Done.
        let lines: Vec<&str> = out.lines().filter(|l| !l.is_empty()).collect();
        assert_eq!(lines.len(), 1, "Done emits exactly one data: line");
        let v: serde_json::Value =
            serde_json::from_str(lines[0].strip_prefix("data: ").unwrap()).unwrap();
        assert_eq!(v["choices"][0]["finish_reason"], "stop");
    }

    #[test]
    fn usage_event_encodes_to_trailing_chunk_with_empty_choices() {
        let out = enc()
            .encode(&LlmStreamEvent::Usage {
                prompt_tokens: 123,
                completion_tokens: 45,
                total_tokens: 168,
                cached_tokens: None,
            })
            .expect("frame");
        let v = parse_data_frame(&out);
        assert_eq!(v["object"], "chat.completion.chunk");
        assert_eq!(v["id"], "chatcmpl-1");
        assert_eq!(v["model"], "test-model");
        assert!(
            v["choices"].as_array().is_some_and(Vec::is_empty),
            "usage chunk must carry an empty choices array, not omit it"
        );
        assert_eq!(v["usage"]["prompt_tokens"], 123);
        assert_eq!(v["usage"]["completion_tokens"], 45);
        assert_eq!(v["usage"]["total_tokens"], 168);
        assert!(
            v["usage"].get("prompt_tokens_details").is_none(),
            "an unreported cached-token count must not synthesize the details object"
        );
    }

    /// A reported count is re-emitted under the OpenAI-standard nesting, so
    /// clients (e.g. the Copilot LLM Gateway extension's `promptTokenDetails`)
    /// see it exactly where they expect.
    #[test]
    fn usage_event_re_emits_a_reported_cached_token_count() {
        let out = enc()
            .encode(&LlmStreamEvent::Usage {
                prompt_tokens: 123,
                completion_tokens: 45,
                total_tokens: 168,
                cached_tokens: Some(100),
            })
            .expect("frame");
        let v = parse_data_frame(&out);
        assert_eq!(v["usage"]["prompt_tokens_details"]["cached_tokens"], 100);
    }

    /// Zero reused tokens is a real measurement, not a missing one, so it must
    /// survive encoding rather than being elided like `None`.
    #[test]
    fn usage_event_distinguishes_zero_cached_tokens_from_absent() {
        let out = enc()
            .encode(&LlmStreamEvent::Usage {
                prompt_tokens: 123,
                completion_tokens: 45,
                total_tokens: 168,
                cached_tokens: Some(0),
            })
            .expect("frame");
        let v = parse_data_frame(&out);
        assert_eq!(v["usage"]["prompt_tokens_details"]["cached_tokens"], 0);
    }

    #[test]
    fn upstream_error_event_encodes_to_bare_error_frame_no_sentinel() {
        let out = enc()
            .encode(&LlmStreamEvent::UpstreamError {
                message: "Context window limit reached.".to_owned(),
                error_type: "context_length_exceeded".to_owned(),
                code: "context_length_exceeded".to_owned(),
            })
            .expect("frame");
        let lines: Vec<&str> = out.lines().filter(|l| !l.is_empty()).collect();
        assert_eq!(lines.len(), 1, "expects only the bare error frame");
        let v: serde_json::Value =
            serde_json::from_str(lines[0].strip_prefix("data: ").unwrap()).unwrap();
        assert_eq!(v["error"]["message"], "Context window limit reached.");
        assert_eq!(v["error"]["type"], "context_length_exceeded");
        assert_eq!(v["error"]["code"], "context_length_exceeded");
        assert!(
            v.get("choices").is_none(),
            "inline error frame must not carry a choices key at all"
        );
        assert!(
            v.get("id").is_none(),
            "inline error frame is deliberately bare, no envelope fields"
        );
    }

    #[test]
    fn prompt_progress_encodes_to_top_level_field() {
        let out = enc()
            .encode(&LlmStreamEvent::PromptProgress {
                processed: 2,
                total: 8,
                cached: 1,
                time_ms: 100,
            })
            .expect("frame");
        let v = parse_data_frame(&out);
        assert_eq!(v["prompt_progress"]["processed"], 2);
        assert_eq!(v["prompt_progress"]["total"], 8);
        assert_eq!(v["prompt_progress"]["cache"], 1);
        assert_eq!(v["prompt_progress"]["time_ms"], 100);
        assert!(v.get("choices").is_none());
    }

    #[test]
    fn normalization_error_is_suppressed() {
        let out = enc().encode(&LlmStreamEvent::NormalizationError {
            kind: NormalizationErrorKind::MalformedToolCallJson {
                raw: "<tool_call>oops".to_owned(),
            },
            raw: "<tool_call>oops".to_owned(),
        });
        assert!(
            out.is_none(),
            "NormalizationError must never reach the wire"
        );
    }
}
