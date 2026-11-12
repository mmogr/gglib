//! What each edit changes in a frame, and what it leaves alone.

use serde_json::json;

use super::*;

/// A content frame as llama-server b12110 writes one.
fn content_frame() -> Value {
    json!({
        "choices": [{"finish_reason": null, "index": 0, "delta": {"content": "hi"}}],
        "created": 1_794_482_104,
        "id": "chatcmpl-x",
        "model": "/models/m.gguf",
        "system_fingerprint": "b12110-cd959bae9",
        "object": "chat.completion.chunk"
    })
}

#[test]
fn the_model_is_named_as_served_and_nothing_else_changes() {
    let mut frame = content_frame();
    assert!(name_served_model(&mut frame, "qwen"));

    let mut want = content_frame();
    want["model"] = json!("qwen");
    assert_eq!(frame, want);
}

#[test]
fn a_frame_that_already_names_the_served_model_is_not_changed() {
    let mut frame = content_frame();
    frame["model"] = json!("qwen");
    assert!(!name_served_model(&mut frame, "qwen"));
}

#[test]
fn a_frame_with_no_model_is_given_none() {
    let mut frame = json!({"choices": []});
    assert!(!name_served_model(&mut frame, "qwen"));
    assert_eq!(frame, json!({"choices": []}));
}

fn usage_frame() -> Value {
    json!({
        "choices": [],
        "usage": {"completion_tokens": 9, "prompt_tokens": 5, "total_tokens": 14},
        "timings": {"prompt_ms": 1.5}
    })
}

#[test]
fn a_reading_joins_the_usage_object() {
    let mut frame = usage_frame();
    assert!(add_reading(&mut frame, ContextReading::new(Some(8192), 3)));

    assert_eq!(
        frame["usage"],
        json!({
            "completion_tokens": 9, "prompt_tokens": 5, "total_tokens": 14,
            "context_size": 8192, "trimmed_messages": 3
        })
    );
    assert_eq!(frame["timings"], json!({"prompt_ms": 1.5}));
}

#[test]
fn a_reading_that_knows_nothing_adds_no_key() {
    let mut frame = usage_frame();
    assert!(!add_reading(&mut frame, ContextReading::default()));
    assert_eq!(frame, usage_frame());
}

#[test]
fn a_frame_with_no_usage_takes_no_reading() {
    let mut frame = content_frame();
    assert!(!add_reading(&mut frame, ContextReading::new(Some(8192), 3)));
    assert_eq!(frame, content_frame());
}

#[test]
fn a_numeric_error_code_becomes_upstream_error_and_the_rest_is_kept() {
    let mut frame = json!({"error": {
        "code": 400, "message": "too long", "type": "exceed_context_size_error", "n_ctx": 4096
    }});
    assert!(name_error_code(
        &mut frame,
        "too long",
        "exceed_context_size_error",
        "upstream_error"
    ));
    assert_eq!(
        frame,
        json!({"error": {
            "code": "upstream_error", "message": "too long",
            "type": "exceed_context_size_error", "n_ctx": 4096
        }})
    );
}

#[test]
fn a_string_error_code_is_kept() {
    let mut frame = json!({"error": {"code": "context_length_exceeded", "message": "m"}});
    assert!(!name_error_code(
        &mut frame,
        "m",
        "server_error",
        "context_length_exceeded"
    ));
}

#[test]
fn an_error_that_is_only_a_string_becomes_an_object() {
    let mut frame = json!({"error": "slot unavailable"});
    assert!(name_error_code(
        &mut frame,
        "slot unavailable",
        "server_error",
        "upstream_error"
    ));
    assert_eq!(
        frame,
        json!({"error": {
            "message": "slot unavailable", "type": "server_error", "code": "upstream_error"
        }})
    );
}

#[test]
fn a_finish_frame_loses_its_tool_calls_and_keeps_its_finish() {
    let mut frame = json!({"choices": [{"finish_reason": "tool_calls", "index": 0, "delta": {
        "tool_calls": [{"index": 0, "function": {"arguments": "}"}}]
    }}]});
    assert!(drop_tool_calls(&mut frame));
    assert_eq!(
        frame,
        json!({"choices": [{"finish_reason": "tool_calls", "index": 0, "delta": {}}]})
    );
    assert!(!drop_tool_calls(&mut frame));
}

#[test]
fn a_block_is_one_data_line_and_a_blank_line() {
    assert_eq!(
        &verbatim("{\"b\":1,\"a\":2}")[..],
        b"data: {\"b\":1,\"a\":2}\n\n"
    );
    assert_eq!(
        &data_block(&json!({"b": 1, "a": 2}))[..],
        b"data: {\"a\":2,\"b\":1}\n\n"
    );
}
