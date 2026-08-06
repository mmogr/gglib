//! One-shot dialect normalization for non-streaming responses.
//!
//! The streaming path runs every response through a [`ToolCallParser`] via
//! [`super::stream::NormalizingStream`]; a `stream: false` request gets the
//! same model, the same dialect, and — until this module — none of the
//! normalization. A Qwen-XML tool call in a non-streaming reply reached the
//! client as raw `<tool_call>` text.
//!
//! [`normalize_chat_completion_body`] closes that gap: it drives the exact
//! same parser (one full-content push, then [`ToolCallParser::finish`]) over
//! each choice's `message.content` and rewrites the body in place — content
//! stripped of markup, extracted calls appended to `message.tool_calls` in
//! the `OpenAI` non-streaming shape, reasoning routed to `reasoning_content`.
//! Chunk-safety is trivially satisfied (the whole body is one chunk), so
//! streaming and non-streaming responses cannot drift: there is one parser
//! per dialect, chosen by the same [`super::registry::get_parser`].

use serde_json::{Value, json};

use super::error::NormalizationError;
use super::parser::ParserOutput;
use super::registry::get_parser;

/// Normalize a complete (non-streaming) `chat.completion` response body in
/// place, using the dialect parser selected by `model_tags`.
///
/// Only `message.content` strings are processed; a null, absent, or
/// non-string content is left untouched, as is everything else in the body.
/// For models with no recognised `format:*` tag the parser is the identity
/// passthrough and the body comes back byte-identical.
///
/// When markup is extracted:
/// - `message.content` becomes the remaining text, or `null` when a tool
///   call consumed all of it (the `OpenAI` shape for tool-call turns);
/// - extracted calls are appended to `message.tool_calls`, `arguments`
///   serialized to a compact JSON string exactly as the streaming encoder
///   does;
/// - reasoning captured by the parser is appended to
///   `message.reasoning_content`;
/// - a `finish_reason` of `stop`/null is upgraded to `"tool_calls"`.
///
/// Returns every [`NormalizationError`] the parser surfaced; the caller
/// decides whether to log them or surface the raw bytes to the client, as
/// the streaming path does.
pub fn normalize_chat_completion_body(
    body: &mut Value,
    model_tags: &[String],
) -> Vec<NormalizationError> {
    let mut all_errors = Vec::new();

    let Some(choices) = body.get_mut("choices").and_then(Value::as_array_mut) else {
        return all_errors;
    };

    for choice in choices {
        let Some(message) = choice.get_mut("message") else {
            continue;
        };
        let Some(content) = message.get("content").and_then(Value::as_str) else {
            continue;
        };
        if content.is_empty() {
            continue;
        }

        // Parsers are stream-stateful: one fresh parser per choice, fed the
        // whole content as a single chunk, then flushed.
        let mut parser = get_parser(model_tags);
        let mut out = parser.push_text(content);
        let fin = parser.finish();
        merge(&mut out, fin);

        // Identity fast-path: nothing extracted, nothing failed, text
        // unchanged — leave the message untouched rather than rebuilding it.
        if out.tool_calls.is_empty()
            && out.errors.is_empty()
            && out.forward_reasoning.is_empty()
            && out.forward_text == content
        {
            continue;
        }

        let extracted_calls = !out.tool_calls.is_empty();

        message["content"] = if out.forward_text.is_empty() && extracted_calls {
            Value::Null
        } else {
            Value::String(out.forward_text)
        };

        if !out.forward_reasoning.is_empty() {
            let merged = match message.get("reasoning_content").and_then(Value::as_str) {
                Some(existing) => format!("{existing}{}", out.forward_reasoning),
                None => out.forward_reasoning,
            };
            message["reasoning_content"] = Value::String(merged);
        }

        if extracted_calls {
            let rendered = out.tool_calls.into_iter().map(|tc| {
                json!({
                    "id": tc.id,
                    "type": "function",
                    "function": {
                        "name": tc.name,
                        // Compact JSON string, matching the streaming
                        // encoder's `arguments.to_string()`.
                        "arguments": tc.arguments.to_string(),
                    },
                })
            });
            match message.get_mut("tool_calls").and_then(Value::as_array_mut) {
                Some(existing) => existing.extend(rendered),
                None => message["tool_calls"] = Value::Array(rendered.collect()),
            }

            // llama-server reported how the *raw* text ended; with the markup
            // rewritten into structured calls, `stop` misdescribes the turn
            // and breaks clients that dispatch on finish_reason.
            let finish = choice.get("finish_reason").and_then(Value::as_str);
            if matches!(finish, None | Some("stop")) {
                choice["finish_reason"] = Value::String("tool_calls".into());
            }
        }

        all_errors.extend(out.errors);
    }

    all_errors
}

/// Fold a second [`ParserOutput`] (from `finish`) into the first.
fn merge(into: &mut ParserOutput, from: ParserOutput) {
    into.forward_text.push_str(&from.forward_text);
    into.forward_reasoning.push_str(&from.forward_reasoning);
    into.tool_calls.extend(from.tool_calls);
    into.errors.extend(from.errors);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::normalize::tags::FORMAT_QWEN_XML;

    fn qwen_tags() -> Vec<String> {
        vec![FORMAT_QWEN_XML.to_owned()]
    }

    fn body_with_content(content: &str) -> Value {
        json!({
            "id": "chatcmpl-1",
            "object": "chat.completion",
            "choices": [{
                "index": 0,
                "message": { "role": "assistant", "content": content },
                "finish_reason": "stop",
            }],
            "usage": { "prompt_tokens": 1, "completion_tokens": 1 },
        })
    }

    /// The gap this module closes: a qwen-xml tool call in a non-streaming
    /// body becomes structured `tool_calls`, not raw text.
    #[test]
    fn qwen_tool_call_markup_becomes_structured_tool_calls() {
        let mut body =
            body_with_content(r#"<tool_call>{"name":"read_file","arguments":{"path":"a.rs"}}</tool_call>"#);
        let errors = normalize_chat_completion_body(&mut body, &qwen_tags());

        assert!(errors.is_empty(), "{errors:?}");
        let message = &body["choices"][0]["message"];
        assert_eq!(message["content"], Value::Null);
        assert_eq!(message["tool_calls"][0]["type"], "function");
        assert_eq!(message["tool_calls"][0]["function"]["name"], "read_file");
        assert_eq!(
            message["tool_calls"][0]["function"]["arguments"],
            r#"{"path":"a.rs"}"#
        );
        assert_eq!(body["choices"][0]["finish_reason"], "tool_calls");
    }

    /// Text around the markup survives as content alongside the calls.
    #[test]
    fn surrounding_text_is_preserved() {
        let mut body = body_with_content(
            r#"On it. <tool_call>{"name":"ls","arguments":{}}</tool_call>"#,
        );
        let errors = normalize_chat_completion_body(&mut body, &qwen_tags());

        assert!(errors.is_empty());
        let message = &body["choices"][0]["message"];
        assert_eq!(message["content"], "On it. ");
        assert_eq!(message["tool_calls"][0]["function"]["name"], "ls");
    }

    /// No recognised tag → identity: the body must come back untouched.
    #[test]
    fn untagged_model_is_passthrough() {
        let original = body_with_content("<tool_call>not for us</tool_call>");
        let mut body = original.clone();
        let errors = normalize_chat_completion_body(&mut body, &[]);

        assert!(errors.is_empty());
        assert_eq!(body, original);
    }

    /// A tagged model whose reply has no markup is also untouched —
    /// including its original `finish_reason`.
    #[test]
    fn plain_text_reply_is_untouched() {
        let original = body_with_content("Just an answer.");
        let mut body = original.clone();
        let errors = normalize_chat_completion_body(&mut body, &qwen_tags());

        assert!(errors.is_empty());
        assert_eq!(body, original);
    }

    /// Malformed markup surfaces as an error for the caller to handle, and
    /// never silently vanishes.
    #[test]
    fn malformed_markup_surfaces_an_error() {
        let mut body = body_with_content("<tool_call>{not json}</tool_call>");
        let errors = normalize_chat_completion_body(&mut body, &qwen_tags());

        assert_eq!(errors.len(), 1);
    }

    /// Null content (already-structured tool-call responses from a --jinja
    /// server) is left alone.
    #[test]
    fn null_content_is_skipped() {
        let mut body = json!({
            "choices": [{
                "message": { "role": "assistant", "content": Value::Null,
                             "tool_calls": [{"id": "x"}] },
                "finish_reason": "tool_calls",
            }],
        });
        let original = body.clone();
        let errors = normalize_chat_completion_body(&mut body, &qwen_tags());

        assert!(errors.is_empty());
        assert_eq!(body, original);
    }
}
