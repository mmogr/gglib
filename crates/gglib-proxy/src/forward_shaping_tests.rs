//! Request shaping at the proxy boundary — tests for [`super`].
//!
//! The transforms themselves are tested in `gglib_core::request_pipeline`.
//! What is left here is the bytes ⇄ JSON conversion unique to the proxy
//! boundary, and the wire contract this surface puts on the pipeline's one
//! failure mode.

use super::*;

fn oversized_body() -> Bytes {
    let mut messages = vec![serde_json::json!({
        "role": "tool", "tool_call_id": "c1", "content": "x".repeat(50_000)
    })];
    for _ in 0..8 {
        messages.push(serde_json::json!({"role": "user", "content": "ok"}));
    }
    Bytes::from(
        serde_json::to_vec(&serde_json::json!({"model": "m", "messages": messages})).unwrap(),
    )
}

#[test]
fn shaping_runs_the_pipeline_and_preserves_unknown_fields() {
    let body = Bytes::from(r#"{"model":"m","messages":[],"totally_made_up":{"a":1}}"#);
    let ShapedRequest {
        body: out,
        truncation: report,
        grammar_enforced,
        ..
    } = shape_request_body(
        body,
        &ModelContext::passthrough(),
        &SamplingLayers::default(),
        None,
    )
    .expect("no budget, so nothing to reject");
    assert!(!grammar_enforced, "passthrough context never constrains");

    let parsed: serde_json::Value = serde_json::from_slice(&out).expect("valid json");
    assert_eq!(parsed["cache_prompt"], true, "the pipeline ran");
    assert!(parsed["temperature"].is_number());
    assert_eq!(parsed["totally_made_up"], serde_json::json!({"a": 1}));
    assert_eq!(report, TruncationReport::default(), "unmeasured, no budget");
}

/// The end-to-end proxy view of the constrain stage: a demanded tool
/// call on a qwen-xml model reports `grammar_enforced` so the log line
/// and dashboard snapshot can say so.
#[test]
fn shaping_reports_grammar_enforcement_for_a_demanded_dialect_call() {
    let ctx = ModelContext {
        tags: vec![gglib_core::normalize::tags::FORMAT_QWEN_XML.to_owned()],
        dialect: Some(gglib_core::domain::DialectSpec::qwen_xml()),
        // Tool-capable, or stage 2b strips the tools before stage 6 sees
        // them — the correct interplay for a model that cannot call tools.
        capabilities: gglib_core::domain::ModelCapabilities::SUPPORTS_TOOL_CALLS,
        catalog_resolved: true,
        ..ModelContext::passthrough()
    };
    let body = Bytes::from(
        r#"{"model":"m","messages":[],"tools":[{"type":"function","function":{"name":"f"}}],"tool_choice":"required"}"#,
    );
    let ShapedRequest {
        body: out,
        grammar_enforced,
        ..
    } = shape_request_body(body, &ctx, &SamplingLayers::default(), None)
        .expect("nothing to reject");

    assert!(grammar_enforced);
    let parsed: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert!(parsed["grammar"].is_string());
    assert_eq!(parsed["tool_choice"], "none");
}

#[test]
fn shaping_leaves_non_json_bodies_alone() {
    let body = Bytes::from_static(b"not json at all");
    let ShapedRequest {
        body: out,
        truncation: report,
        ..
    } = shape_request_body(
        body.clone(),
        &ModelContext::passthrough(),
        &SamplingLayers::default(),
        Some(10),
    )
    .expect("a body we cannot read is forwarded, not rejected");

    assert_eq!(out, body, "must be the same bytes, not the same value");
    assert_eq!(report, TruncationReport::default());
}

#[test]
fn shaping_truncates_when_the_budget_binds() {
    let ShapedRequest {
        body: out,
        truncation: report,
        ..
    } = shape_request_body(
        oversized_body(),
        &ModelContext::passthrough(),
        &SamplingLayers::default(),
        Some(20_000),
    )
    .expect("trimming the one oversized tool result is enough");

    assert_eq!(report.messages_truncated, 1);
    assert!(report.payload_chars_after <= 20_000);
    assert!(out.len() <= 20_000);
}

#[test]
fn shaping_reports_the_error_when_the_budget_cannot_be_met() {
    let err = shape_request_body(
        oversized_body(),
        &ModelContext::passthrough(),
        &SamplingLayers::default(),
        Some(200),
    )
    .expect_err("nothing left to trim, still over");

    assert!(matches!(
        err,
        TruncationError::ExceedsBudgetAfterTruncation { .. }
    ));
}

/// The wire contract clients branch on. Asserted field by field because
/// this is a public interface of the proxy, not an internal detail: the
/// status, both codes and the message are all load-bearing.
#[tokio::test]
async fn the_context_length_contract_is_400_with_both_codes_set() {
    let response = context_length_exceeded_response();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body reads");
    let parsed: serde_json::Value = serde_json::from_slice(&bytes).expect("valid json");

    assert_eq!(parsed["error"]["type"], "context_length_exceeded");
    assert_eq!(parsed["error"]["code"], "context_length_exceeded");
    assert_eq!(
        parsed["error"]["message"],
        "Context window limit reached. Please start a new conversation."
    );
}
