//! Tests for [`super::truncate_history`].
//!
//! Split out via `#[path]` so the stage itself stays inside the file budget.

use super::*;
use serde_json::json;

// ── Builders ─────────────────────────────────────────────────────────────────

fn body(messages: &[Value]) -> Value {
    json!({"model": "test-model", "messages": messages})
}

fn msg(role: &str, content: &str) -> Value {
    json!({"role": role, "content": content})
}

fn big(n: usize) -> String {
    "x".repeat(n)
}

/// Pad a message list out past [`PROTECTED_TAIL_COUNT`] so the leading entries
/// are actually eligible for trimming.
fn with_tail(mut messages: Vec<Value>) -> Vec<Value> {
    for _ in 0..PROTECTED_TAIL_COUNT {
        messages.push(msg("user", "ok"));
    }
    messages
}

/// A budget with room to spare for every fixture here.
const ROOMY: usize = 240_000;

// ── Budget gate ──────────────────────────────────────────────────────────────

#[test]
fn under_budget_leaves_the_body_completely_untouched() {
    let mut b = body(&[msg("user", "hello"), msg("assistant", "world")]);
    let before = b.clone();

    let report = truncate_history(&mut b, ROOMY).unwrap();

    assert_eq!(b, before, "the body must not be modified under budget");
    assert_eq!(report.messages_truncated, 0);
    assert_eq!(report.payload_chars_before, report.payload_chars_after);
}

#[test]
fn under_budget_leaves_oversized_content_intact() {
    // Behavioural guard: with room in the budget, even a giant tool output is
    // forwarded untouched — history is not mutilated pre-emptively.
    let mut b = body(&with_tail(vec![msg(
        "tool",
        &big(TOOL_CONTENT_THRESHOLD_CHARS * 10),
    )]));
    let before = b.clone();

    let report = truncate_history(&mut b, ROOMY).unwrap();

    assert_eq!(report.messages_truncated, 0, "nothing trimmed under budget");
    assert_eq!(b, before);
}

#[test]
fn missing_messages_field_passes_through_unchanged() {
    // Zero blast radius: a body this stage cannot read is forwarded, not
    // rejected, even when it is over budget.
    let mut b = json!({"model": "test", "blob": big(5_000)});
    let before = b.clone();

    let report = truncate_history(&mut b, 1_000).unwrap();

    assert_eq!(b, before);
    assert_eq!(report.messages_truncated, 0);
}

// ── Oldest-first trimming ────────────────────────────────────────────────────

#[test]
fn over_budget_trims_oldest_first_and_stops_early() {
    // Four 100k tool messages outside the protected tail. The payload (~400k)
    // exceeds the 240k budget; trimming the two oldest brings it back under, so
    // the two newer ones must survive.
    let mut b = body(&with_tail(vec![
        msg("tool", &big(100_000)),
        msg("tool", &big(100_000)),
        msg("tool", &big(100_000)),
        msg("tool", &big(100_000)),
    ]));

    let report = truncate_history(&mut b, ROOMY).unwrap();

    assert_eq!(
        report.messages_truncated, 2,
        "only as many oldest messages as needed to get under budget"
    );
    assert_eq!(b["messages"][0]["content"], TRUNCATION_PLACEHOLDER);
    assert_eq!(b["messages"][1]["content"], TRUNCATION_PLACEHOLDER);
    assert_eq!(
        b["messages"][2]["content"].as_str().unwrap().len(),
        100_000,
        "newer big message preserved once under budget"
    );
    assert_eq!(b["messages"][3]["content"].as_str().unwrap().len(), 100_000);
    assert!(report.payload_chars_after <= ROOMY);
    assert!(report.payload_chars_after < report.payload_chars_before);
}

#[test]
fn assistant_content_is_an_eligible_candidate() {
    let mut b = body(&with_tail(vec![
        msg("assistant", &big(150_000)),
        msg("assistant", &big(150_000)),
    ]));

    let report = truncate_history(&mut b, ROOMY).unwrap();

    assert!(report.messages_truncated >= 1);
}

// ── Protection ───────────────────────────────────────────────────────────────

#[test]
fn system_messages_are_never_truncated() {
    let mut b = body(&with_tail(vec![
        json!({"role": "system", "content": big(120_000)}),
        msg("tool", &big(120_000)),
        msg("tool", &big(120_000)),
    ]));

    truncate_history(&mut b, ROOMY).unwrap();

    assert_eq!(
        b["messages"][0]["content"].as_str().unwrap().len(),
        120_000,
        "system content must survive"
    );
}

/// The property that makes truncation safe: the model always keeps its most
/// recent turns, however tight the budget.
#[test]
fn the_protected_tail_is_never_trimmed() {
    // One huge unprotected tool at index 0, then two oversized tools that sit
    // inside the protected tail. Trimming index 0 alone drops the payload under
    // budget, and the tail must be untouched regardless.
    let mut b = body(&with_tail(vec![
        msg("tool", &big(300_000)),
        msg("tool", &big(30_000)),
        msg("tool", &big(30_000)),
    ]));

    let report = truncate_history(&mut b, ROOMY).unwrap();

    assert_eq!(b["messages"][0]["content"], TRUNCATION_PLACEHOLDER);
    assert_eq!(b["messages"][1]["content"].as_str().unwrap().len(), 30_000);
    assert_eq!(b["messages"][2]["content"].as_str().unwrap().len(), 30_000);
    assert_eq!(report.messages_truncated, 1);
}

/// Every message is inside the tail window when there are fewer than
/// [`PROTECTED_TAIL_COUNT`] of them — a short conversation is wholly immune.
#[test]
fn a_short_conversation_is_entirely_protected() {
    let mut b = body(&[msg("tool", &big(50_000)), msg("user", "go")]);
    let before = b.clone();

    let err = truncate_history(&mut b, 1_000).unwrap_err();

    assert_eq!(b, before, "nothing was eligible, so nothing changed");
    assert!(matches!(
        err,
        TruncationError::ExceedsBudgetAfterTruncation { .. }
    ));
}

// ── Hard abort ───────────────────────────────────────────────────────────────

#[test]
fn still_over_budget_after_trimming_everything_is_an_error() {
    // A system prompt so large that trimming the one eligible tool message
    // cannot bring the payload back under budget.
    let mut b = body(&with_tail(vec![
        json!({"role": "system", "content": big(300_000)}),
        msg("tool", &big(50_000)),
    ]));

    let err = truncate_history(&mut b, ROOMY).unwrap_err();

    let TruncationError::ExceedsBudgetAfterTruncation {
        payload_chars,
        limit_chars,
    } = err;
    assert_eq!(limit_chars, ROOMY);
    assert!(payload_chars > ROOMY);
    assert_eq!(
        b["messages"][1]["content"], TRUNCATION_PLACEHOLDER,
        "the eligible message was still trimmed before giving up"
    );
}

#[test]
fn sub_threshold_content_is_not_trimmed_even_over_budget() {
    // Many small tool messages exceed the budget by sheer count, but none is
    // individually over the per-message threshold, so none is eligible.
    let messages: Vec<Value> = (0..300)
        .map(|_| msg("tool", &big(TOOL_CONTENT_THRESHOLD_CHARS - 1)))
        .collect();

    assert!(truncate_history(&mut body(&messages), ROOMY).is_err());
}

// ── Content forms ────────────────────────────────────────────────────────────

#[test]
fn array_form_content_is_skipped() {
    let mut b = body(&with_tail(vec![
        json!({"role": "tool", "tool_call_id": "c1", "content": big(250_000)}),
        json!({"role": "tool", "tool_call_id": "c2", "content": [{"type": "text", "text": "hi"}]}),
    ]));

    let report = truncate_history(&mut b, ROOMY).unwrap();

    assert_eq!(report.messages_truncated, 1, "only the string-form message");
    assert_eq!(b["messages"][0]["content"], TRUNCATION_PLACEHOLDER);
    assert!(b["messages"][1]["content"].is_array());
}

#[test]
fn an_assistant_turn_without_content_is_left_alone() {
    let mut b = body(&with_tail(vec![json!({
        "role": "assistant",
        "tool_calls": [{"id": "c1", "type": "function", "function": {"name": "f", "arguments": "{}"}}]
    })]));
    let before = b.clone();

    assert!(truncate_history(&mut b, 100).is_err());
    assert_eq!(b, before);
}

#[test]
fn tool_calls_survive_when_content_is_truncated() {
    let mut b = body(&with_tail(vec![json!({
        "role": "assistant",
        "content": big(250_000),
        "tool_calls": [{"id": "call_1", "type": "function", "function": {"name": "foo", "arguments": "{}"}}]
    })]));

    let report = truncate_history(&mut b, ROOMY).unwrap();

    assert_eq!(report.messages_truncated, 1);
    assert_eq!(b["messages"][0]["content"], TRUNCATION_PLACEHOLDER);
    assert_eq!(b["messages"][0]["tool_calls"][0]["id"], "call_1");
}

// ── The budget is the model's, with no floor ─────────────────────────────────

/// The whole point of dropping `TOTAL_PAYLOAD_LIMIT_CHARS`: a small-context
/// model gets a small budget, and it is enforced. Under the old 240,000-char
/// floor this body was forwarded whole.
#[test]
fn a_small_model_budget_is_honoured_rather_than_floored() {
    let messages = with_tail(vec![
        msg("tool", &big(10_000)),
        msg("tool", &big(10_000)),
        msg("tool", &big(10_000)),
    ]);
    // 4096 tokens × CHARS_PER_TOKEN_APPROX.
    let budget = 4_096 * CHARS_PER_TOKEN_APPROX;

    let mut b = body(&messages);
    let report = truncate_history(&mut b, budget).unwrap();

    assert!(report.messages_truncated > 0, "a 16k budget must bite");
    assert!(report.payload_chars_after <= budget);

    // The very same conversation on a large-context model is left alone.
    let mut roomy = body(&messages);
    let before = roomy.clone();
    let report = truncate_history(&mut roomy, 262_144 * CHARS_PER_TOKEN_APPROX).unwrap();
    assert_eq!(report.messages_truncated, 0);
    assert_eq!(roomy, before);
}

#[test]
fn a_large_budget_admits_a_payload_the_old_floor_would_have_allowed_anyway() {
    // A protected system prompt bigger than the historical 240,000 floor, well
    // within a 131k-context model's budget.
    let mut b = body(&[
        json!({"role": "system", "content": big(250_000)}),
        msg("user", "go"),
    ]);

    assert!(truncate_history(&mut b, 131_072 * CHARS_PER_TOKEN_APPROX).is_ok());
}
