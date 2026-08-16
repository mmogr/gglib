//! Tests for [`super::apply`].
//!
//! Split out via `#[path]` so the module itself stays inside the file budget.

use super::*;
use crate::domain::{InferenceConfig, ModelCapabilities};
use serde_json::json;

fn strict_turn_ctx() -> ModelContext {
    ModelContext {
        capabilities: ModelCapabilities::REQUIRES_STRICT_TURNS,
        inference_defaults: Some(InferenceConfig {
            temperature: Some(0.33),
            ..Default::default()
        }),
        ..ModelContext::passthrough()
    }
}

fn kitchen_sink() -> Value {
    json!({
        "model": "m",
        "cache_prompt": false,
        "messages": [
            {"role": "assistant", "content": "<think>x</think>a", "reasoning_content": "r"},
            {"role": "assistant", "content": "b"},
            {"role": "tool", "tool_call_id": "call_1", "content": "result"},
        ],
        "totally_made_up_key": {"nested": [1, 2]},
    })
}

/// Stage 1 must have already run when stage 2 merges: the merged text
/// contains no `<think>` remnant, which it would if the order were flipped.
#[test]
fn reasoning_is_stripped_before_messages_are_merged() {
    let mut body = kitchen_sink();
    apply(
        &mut body,
        &strict_turn_ctx(),
        &SamplingLayers::default(),
        None,
    )
    .unwrap();

    let merged = body["messages"][0]["content"].as_str().unwrap();
    assert_eq!(merged, "a\n\nb");
    assert!(body["messages"][0].get("reasoning_content").is_none());
}

#[test]
fn every_stage_runs_in_one_call() {
    let mut body = kitchen_sink();
    let report = apply(
        &mut body,
        &strict_turn_ctx(),
        &SamplingLayers::default(),
        Some(100_000),
    )
    .unwrap();

    // 1 + 2: reasoning gone, assistant turns merged, tool turn intact.
    assert_eq!(body["messages"].as_array().unwrap().len(), 2);
    assert_eq!(body["messages"][1]["tool_call_id"], "call_1");
    // 3: measured, nothing to trim.
    assert_eq!(report.truncation.messages_truncated, 0);
    assert!(report.truncation.payload_chars_before > 0);
    // 4: the model's stored default resolved in.
    assert!((body["temperature"].as_f64().unwrap() - 0.33).abs() < 1e-6);
    // 5: pinned over the client's explicit `false`.
    assert_eq!(body["cache_prompt"], true);
    // …and nothing else was disturbed.
    assert_eq!(body["model"], "m");
    assert_eq!(body["totally_made_up_key"], json!({"nested": [1, 2]}));
}

/// A passthrough context must cost the request nothing but its
/// model-specific handling — the sampling stages still run.
#[test]
fn a_passthrough_context_still_resolves_sampling() {
    let mut body = json!({"messages": [
        {"role": "user", "content": "one"},
        {"role": "user", "content": "two"},
    ]});
    apply(
        &mut body,
        &ModelContext::passthrough(),
        &SamplingLayers::default(),
        None,
    )
    .unwrap();

    assert_eq!(
        body["messages"].as_array().unwrap().len(),
        2,
        "unknown capabilities must not merge anything"
    );
    assert_eq!(body["cache_prompt"], true);
    assert!(body["temperature"].as_f64().is_some());
}

// ── Stage 3 ──────────────────────────────────────────────────────────────

fn oversized_body() -> Value {
    let mut messages = vec![json!({"role": "tool", "content": "x".repeat(50_000)})];
    for _ in 0..8 {
        messages.push(json!({"role": "user", "content": "ok"}));
    }
    json!({"model": "m", "messages": messages})
}

#[test]
fn an_oversized_conversation_is_trimmed() {
    let mut body = oversized_body();
    let report = apply(
        &mut body,
        &ModelContext::passthrough(),
        &SamplingLayers::default(),
        Some(20_000),
    )
    .unwrap();

    assert_eq!(report.truncation.messages_truncated, 1);
    assert!(report.truncation.payload_chars_after <= 20_000);
}

/// No budget means no measurement — not a zero budget that rejects
/// everything.
#[test]
fn no_budget_means_no_truncation() {
    let mut body = oversized_body();
    let report = apply(
        &mut body,
        &ModelContext::passthrough(),
        &SamplingLayers::default(),
        None,
    )
    .unwrap();

    assert_eq!(report.truncation, TruncationReport::default());
    assert_eq!(
        body["messages"][0]["content"].as_str().unwrap().len(),
        50_000
    );
}

/// Stage 3 runs before stage 4, so the budget is measured against the
/// client's conversation and not against sampling keys we added ourselves.
#[test]
fn sampling_keys_are_not_counted_against_the_budget() {
    let mut body = oversized_body();
    // Small enough that even the fully-trimmed conversation cannot fit, so
    // the run stops at stage 3 with stage 4 still ahead of it.
    let err = apply(
        &mut body,
        &ModelContext::passthrough(),
        &SamplingLayers::default(),
        Some(200),
    )
    .unwrap_err();

    let TruncationError::ExceedsBudgetAfterTruncation { payload_chars, .. } = err;
    assert!(
        body.get("temperature").is_none(),
        "stage 4 must not have run before the measurement that rejected this"
    );
    assert!(payload_chars > 200);
}
