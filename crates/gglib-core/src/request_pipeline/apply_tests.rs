//! Tests for [`super::apply`].
//!
//! Split out via `#[path]` so the module itself stays inside the file budget.

use super::super::AGENTIC_NOTE;
use super::super::tests_support::chars;
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
        Some(chars(100_000)),
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
        Some(chars(20_000)),
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
        Some(chars(200)),
    )
    .unwrap_err();

    let TruncationError::ExceedsBudgetAfterTruncation { payload_chars, .. } = err;
    assert!(
        body.get("temperature").is_none(),
        "stage 4 must not have run before the measurement that rejected this"
    );
    assert!(payload_chars > 200);
}

// ── Stage 1c ─────────────────────────────────────────────────────────────

fn tool_turn(messages: &Value) -> Value {
    json!({
        "model": "m",
        "messages": messages,
        "tools": [{ "type": "function", "function": { "name": "read_file" } }],
    })
}

/// The layers of the one caller that turns the batching note on.
fn noting() -> SamplingLayers {
    SamplingLayers {
        agentic_prompt_note: true,
        ..SamplingLayers::default()
    }
}

/// How many times the batching note appears anywhere in `body`.
fn batching_notes(body: &Value) -> usize {
    let escaped = Value::from(AGENTIC_NOTE).to_string();
    body.to_string().matches(escaped.trim_matches('"')).count()
}

/// Stage 1c runs before stage 2: a model with no system role reads the note
/// where the pipeline puts any system text, in a `[System]:` user message,
/// and no `system` message is left. Shaped again, the body does not change.
#[test]
fn a_model_with_no_system_role_reads_the_note_as_user_text() {
    let strict = ModelContext {
        capabilities: ModelCapabilities::REQUIRES_STRICT_TURNS,
        ..ModelContext::passthrough()
    };
    for messages in [
        json!([{"role": "system", "content": "Be brief."}, {"role": "user", "content": "go"}]),
        json!([{"role": "user", "content": "go"}]),
    ] {
        let mut shaped = tool_turn(&messages);
        apply(&mut shaped, &strict, &noting(), None).unwrap();

        let sent = shaped["messages"].as_array().unwrap();
        assert!(sent.iter().all(|message| message["role"] != "system"));
        assert_eq!(sent[0]["role"], "user");
        let first = sent[0]["content"].as_str().unwrap();
        assert!(first.starts_with("[System]: "), "{first}");
        assert!(first.contains(AGENTIC_NOTE), "{first}");
        assert_eq!(batching_notes(&shaped), 1);

        let once = shaped["messages"].to_string();
        apply(&mut shaped, &strict, &noting(), None).unwrap();
        assert_eq!(shaped["messages"].to_string(), once, "shaped twice");
    }
}

/// Stage 1c runs before stage 3: the same request measures longer with the
/// note by exactly the bytes the note adds to it.
#[test]
fn the_pipeline_measures_the_note_with_the_request() {
    let body = tool_turn(&json!([
        {"role": "system", "content": "Be brief."},
        {"role": "user", "content": "go"},
    ]));
    let budget = Some(chars(100_000));

    let mut untold = body.clone();
    let without = apply(
        &mut untold,
        &ModelContext::passthrough(),
        &SamplingLayers::default(),
        budget,
    )
    .unwrap();
    let mut told = body;
    let with = apply(&mut told, &ModelContext::passthrough(), &noting(), budget).unwrap();
    assert_eq!(batching_notes(&told), 1);

    let added = Value::from(format!("\n\n{AGENTIC_NOTE}")).to_string().len() - "\"\"".len();
    assert_eq!(
        with.truncation.payload_chars_before,
        without.truncation.payload_chars_before + added
    );
}
