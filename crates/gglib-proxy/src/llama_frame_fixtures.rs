//! Upstream bytes for the drain tests, shaped as llama-server b12110 writes
//! them: every frame in the full envelope, a progress frame with `choices`,
//! the usage frame with `timings`, and a mid-stream error with a numeric
//! code. One frame for each event, so a test that counts frames counts
//! events.

use bytes::Bytes;
use serde_json::{Value, json};

use gglib_core::LlmStreamEvent;

/// The `id` every frame here carries.
pub(crate) const UPSTREAM_ID: &str = "chatcmpl-upstream";

/// The `model` every frame here carries: a GGUF path, as llama-server names
/// a model it was given with `-m`.
pub(crate) const UPSTREAM_MODEL: &str = "/models/m.gguf";

/// `body` in llama-server's envelope, written at `created`.
fn envelope(body: Value, created: u64) -> Value {
    let mut frame = json!({
        "created": created,
        "id": UPSTREAM_ID,
        "model": UPSTREAM_MODEL,
        "system_fingerprint": "b12110-cd959bae9",
        "object": "chat.completion.chunk",
    });
    if let (Some(frame), Value::Object(body)) = (frame.as_object_mut(), body) {
        frame.extend(body);
    }
    frame
}

/// One choice with `delta` and `finish_reason`, as llama-server orders it.
fn choice(delta: &Value, finish_reason: Option<&str>) -> Value {
    json!({ "choices": [{ "finish_reason": finish_reason, "index": 0, "delta": delta }] })
}

/// The frame llama-server writes for `event`, at `created`.
///
/// # Panics
///
/// On [`LlmStreamEvent::WritingTime`], which no frame carries.
pub(crate) fn llama_frame_at(event: &LlmStreamEvent, created: u64) -> Value {
    match event {
        LlmStreamEvent::TextDelta { content } => {
            envelope(choice(&json!({ "content": content }), None), created)
        }
        LlmStreamEvent::ReasoningDelta { content } => envelope(
            choice(&json!({ "reasoning_content": content }), None),
            created,
        ),
        LlmStreamEvent::ToolCallDelta {
            index,
            id,
            name,
            arguments,
        } => {
            let mut call = json!({ "index": index });
            if let Some(id) = id {
                call["id"] = json!(id);
                call["type"] = json!("function");
            }
            let mut function = json!({});
            if let Some(name) = name {
                function["name"] = json!(name);
            }
            if let Some(arguments) = arguments {
                function["arguments"] = json!(arguments);
            }
            call["function"] = function;
            envelope(choice(&json!({ "tool_calls": [call] }), None), created)
        }
        LlmStreamEvent::Done { finish_reason } => {
            envelope(choice(&json!({}), finish_reason.as_deref()), created)
        }
        LlmStreamEvent::Usage {
            prompt_tokens,
            completion_tokens,
            total_tokens,
            cached_tokens,
        } => {
            let mut usage = json!({
                "completion_tokens": completion_tokens,
                "prompt_tokens": prompt_tokens,
                "total_tokens": total_tokens,
            });
            if let Some(cached) = cached_tokens {
                usage["prompt_tokens_details"] = json!({ "cached_tokens": cached });
            }
            let body =
                json!({ "choices": [], "usage": usage, "timings": { "prompt_n": prompt_tokens } });
            envelope(body, created)
        }
        LlmStreamEvent::PromptProgress {
            processed,
            total,
            cached,
            time_ms,
        } => {
            let mut body = choice(&json!({ "role": "assistant", "content": null }), None);
            body["prompt_progress"] = json!({
                "total": total, "cache": cached, "processed": processed, "time_ms": time_ms,
            });
            envelope(body, created)
        }
        LlmStreamEvent::UpstreamError {
            message,
            error_type,
            ..
        } => json!({ "error": { "code": 500, "message": message, "type": error_type } }),
        LlmStreamEvent::WritingTime { .. } => panic!("no frame carries a writing time"),
    }
}

/// [`llama_frame_at`], at one fixed time.
pub(crate) fn llama_frame(event: &LlmStreamEvent) -> Value {
    llama_frame_at(event, 1_794_482_104)
}

/// `frame` as the bytes of one SSE block.
pub(crate) fn sse(frame: &Value) -> Bytes {
    Bytes::from(format!("data: {frame}\n\n"))
}

/// The bytes llama-server sends for `event`.
pub(crate) fn llama_bytes(event: &LlmStreamEvent) -> Bytes {
    sse(&llama_frame(event))
}

/// The role frame llama-server opens a reply with.
pub(crate) fn role_bytes() -> Bytes {
    sse(&envelope(
        choice(&json!({ "role": "assistant", "content": null }), None),
        1_794_482_104,
    ))
}

/// The sentinel llama-server ends its body with.
pub(crate) fn done_bytes() -> Bytes {
    Bytes::from_static(b"data: [DONE]\n\n")
}
