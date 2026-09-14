//! Tests for [`super`]: whether a turn is re-issued, and which response wins.
//!
//! Moved out of `repair.rs` as they were, to keep that file within the Rust
//! size ratchet. The accumulator and event synthesis tests are in
//! `repair_stream_tests.rs`, a child of this module that shares its fixtures.

use super::*;
use gglib_core::domain::{DialectSpec, ModelCapabilities};
use gglib_core::request_pipeline::{ModelContext, SamplingLayers};
use serde_json::json;

fn request(tool_choice: Value) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "model": "m",
        "messages": [{"role": "user", "content": "read it"}],
        "tool_choice": tool_choice,
        "tools": [{
            "type": "function",
            "function": {
                "name": "read_file",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": {"type": "string"},
                        "max_lines": {"type": "integer"}
                    },
                    "required": ["path"]
                }
            }
        }]
    }))
    .unwrap()
}

fn response(arguments: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "tool_calls": [{
                    "type": "function",
                    "function": {"name": "read_file", "arguments": arguments}
                }]
            }
        }]
    }))
    .unwrap()
}

/// The measured Llama 3.2 failure: an integer field carrying a string.
#[test]
fn a_schema_violation_on_the_auto_path_is_reissued() {
    let d = decide(
        &request(json!("auto")),
        &response(r#"{"path":"a","max_lines":"42"}"#),
        RepairTurn::ON,
    );

    let Decision::Reissue { body, violations } = d else {
        panic!("expected a re-issue, got {d:?}");
    };
    assert!(violations[0].contains("max_lines"), "{violations:?}");

    let parsed: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed["tool_choice"], "required");
    assert_eq!(
        parsed["messages"],
        request_value()["messages"],
        "prefix unchanged"
    );
}

fn request_value() -> Value {
    serde_json::from_slice(&request(json!("auto"))).unwrap()
}

/// The repair must go out non-streaming, or a second SSE pipeline would
/// have to run inside the first.
#[test]
fn the_repair_request_is_non_streaming() {
    let Decision::Reissue { body, .. } = decide(
        &request(json!("auto")),
        &response(r#"{"path":"a","max_lines":"42"}"#),
        RepairTurn::ON,
    ) else {
        panic!("expected a re-issue");
    };

    let parsed: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed["stream"], false);
    assert!(parsed.get("stream_options").is_none());
}

#[test]
fn a_conformant_call_is_forwarded() {
    assert_eq!(
        decide(
            &request(json!("auto")),
            &response(r#"{"path":"a"}"#),
            RepairTurn::ON
        ),
        Decision::Forward(Skipped::Conformant)
    );
}

/// Re-issuing with `required` when the client already asked for it would
/// reproduce the same failure at full cost.
#[test]
fn an_already_required_request_is_not_reissued() {
    assert_eq!(
        decide(
            &request(json!("required")),
            &response(r#"{"path":"a","max_lines":"42"}"#),
            RepairTurn::ON
        ),
        Decision::Forward(Skipped::AlreadyConstrained)
    );
}

/// Absent `tool_choice` is `auto` per the OpenAI contract, and is what
/// most clients actually send.
#[test]
fn an_absent_tool_choice_counts_as_auto() {
    let d = decide(
        &request(json!(null)),
        &response(r#"{"path":"a","max_lines":"42"}"#),
        RepairTurn::ON,
    );
    assert!(matches!(d, Decision::Reissue { .. }));
}

#[test]
fn disabling_repair_forwards_everything() {
    assert_eq!(
        decide(
            &request(json!("auto")),
            &response(r#"{"path":"a","max_lines":"42"}"#),
            RepairTurn::OFF
        ),
        Decision::Forward(Skipped::Disabled)
    );
}

#[test]
fn an_unreadable_body_is_forwarded() {
    assert_eq!(
        decide(b"not json", &response(r#"{"path":"a"}"#), RepairTurn::ON),
        Decision::Forward(Skipped::Unreadable)
    );
}

#[test]
fn a_response_without_tool_calls_is_not_applicable() {
    let plain = serde_json::to_vec(&json!({
        "choices": [{"message": {"role": "assistant", "content": "hello"}}]
    }))
    .unwrap();

    assert_eq!(
        decide(&request(json!("auto")), &plain, RepairTurn::ON),
        Decision::Forward(Skipped::NotApplicable)
    );
}

#[test]
fn a_conformant_repair_replaces_the_original() {
    let original = Bytes::from(response(r#"{"path":"a","max_lines":"42"}"#));
    let repaired = Bytes::from(response(r#"{"path":"a","max_lines":42}"#));

    let (chosen, did_repair) = choose(&request(json!("auto")), original, repaired.clone());

    assert!(did_repair);
    assert_eq!(chosen, repaired);
}

/// Fail-open: a repair that is still wrong must never be forwarded in
/// place of the original.
#[test]
fn a_still_invalid_repair_is_discarded() {
    let original = Bytes::from(response(r#"{"path":"a","max_lines":"42"}"#));
    let repaired = Bytes::from(response(r#"{"path":"a","max_lines":"still bad"}"#));

    let (chosen, did_repair) = choose(&request(json!("auto")), original.clone(), repaired);

    assert!(!did_repair);
    assert_eq!(chosen, original);
}

#[test]
fn an_unreadable_repair_is_discarded() {
    let original = Bytes::from(response(r#"{"path":"a"}"#));
    let (chosen, did_repair) = choose(
        &request(json!("auto")),
        original.clone(),
        Bytes::from_static(b"garbage"),
    );

    assert!(!did_repair);
    assert_eq!(chosen, original);
}

/// Why a repair body must never be sent back through the request
/// pipeline.
///
/// Stage 6 fires on `tool_choice: "required"` for a dialect model,
/// installs gglib's own grammar and rewrites `tool_choice` to `"none"` —
/// llama-server rejects a custom grammar alongside `tools`. Applied to a
/// repair that silently converts the re-issue into a request for no tool
/// call at all: a full generation spent, nothing changed, no error
/// anywhere.
///
/// `repair_body` avoids this by construction — it mutates the
/// already-resolved body and sends it, never calling `apply`. This test
/// demonstrates the damage that bypass prevents, so the reason survives
/// as something executable rather than as a comment nobody re-derives.
#[test]
fn the_pipeline_would_destroy_a_repair_body_which_is_why_it_bypasses_it() {
    let ctx = ModelContext {
        capabilities: ModelCapabilities::SUPPORTS_TOOL_CALLS,
        catalog_resolved: true,
        dialect: Some(DialectSpec::qwen_xml()),
        ..ModelContext::passthrough()
    };

    let Decision::Reissue { body, .. } = decide(
        &request(json!("auto")),
        &response(r#"{"path":"a","max_lines":"42"}"#),
        RepairTurn::ON,
    ) else {
        panic!("expected a re-issue");
    };

    // What `repair_body` actually produces, and sends as-is.
    let as_sent: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        as_sent["tool_choice"], "required",
        "the re-issue must reach llama-server still demanding a call"
    );
    assert!(
        as_sent.get("grammar").is_none(),
        "gglib must not install its own weaker grammar on the repair path"
    );

    // The same body put through the pipeline — the thing that must not
    // happen. If this ever stops rewriting `tool_choice`, the bypass is
    // no longer load-bearing and this test should be revisited.
    let mut through_pipeline: Value = serde_json::from_slice(&body).unwrap();
    gglib_core::request_pipeline::apply(
        &mut through_pipeline,
        &ctx,
        &SamplingLayers::default(),
        None,
    )
    .unwrap();
    assert_eq!(
        through_pipeline["tool_choice"], "none",
        "stage 6 would ask for no tool call at all — hence the bypass"
    );
}

#[path = "repair_stream_tests.rs"]
mod repair_stream_tests;
