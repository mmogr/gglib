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
    // exceeds the 240k budget; the quantized watermark target (240,000 saved
    // chars here) takes the three oldest, so the newest must survive.
    let mut b = body(&with_tail(vec![
        msg("tool", &big(100_000)),
        msg("tool", &big(100_000)),
        msg("tool", &big(100_000)),
        msg("tool", &big(100_000)),
    ]));

    let report = truncate_history(&mut b, ROOMY).unwrap();

    assert_eq!(
        report.messages_truncated, 3,
        "as many oldest messages as the watermark target needs"
    );
    assert_eq!(b["messages"][0]["content"], TRUNCATION_PLACEHOLDER);
    assert_eq!(b["messages"][1]["content"], TRUNCATION_PLACEHOLDER);
    assert_eq!(b["messages"][2]["content"], TRUNCATION_PLACEHOLDER);
    assert_eq!(
        b["messages"][3]["content"].as_str().unwrap().len(),
        100_000,
        "newest big message preserved once the target is met"
    );
    assert!(report.payload_chars_after <= ROOMY * LOW_WATERMARK_PCT / 100);
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
    // One huge tool at index 0, then two oversized tools at indices 1-2. With
    // eleven messages the protected window is indices 3..=10, so all three are
    // eligible — but index 0 alone covers the savings target, and everything
    // behind it survives, protected tail included.
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

// ── Low-watermark hysteresis ─────────────────────────────────────────────────

#[test]
fn a_payload_in_the_dead_zone_is_left_untouched() {
    // Two 100k tool messages land the payload (~200k) between the 180k
    // watermark and the 240k budget. Over the watermark is not a trigger —
    // only over the budget is — so nothing moves and the prompt prefix
    // survives verbatim.
    let mut b = body(&with_tail(vec![
        msg("tool", &big(100_000)),
        msg("tool", &big(100_000)),
    ]));
    let before = b.clone();

    let report = truncate_history(&mut b, ROOMY).unwrap();

    assert_eq!(b, before, "no trimming inside the dead zone");
    assert_eq!(report.messages_truncated, 0);
    assert_eq!(report.payload_chars_before, report.payload_chars_after);
    // The fixture really is inside the zone, not merely under the budget.
    assert!(report.payload_chars_before > ROOMY * LOW_WATERMARK_PCT / 100);
    assert!(report.payload_chars_before <= ROOMY);
}

#[test]
fn a_triggered_trim_lands_at_the_watermark_not_barely_under_budget() {
    // Same fixture as `over_budget_trims_oldest_first_and_stops_early`:
    // minimal-fit trimming would stop after two messages, just under the 240k
    // budget, and then move the elision frontier again on the very next turn.
    // The watermark target takes a third, parking the payload under 75% of
    // budget so following turns need no new elisions.
    let mut b = body(&with_tail(vec![
        msg("tool", &big(100_000)),
        msg("tool", &big(100_000)),
        msg("tool", &big(100_000)),
        msg("tool", &big(100_000)),
    ]));

    let report = truncate_history(&mut b, ROOMY).unwrap();

    assert_eq!(report.messages_truncated, 3, "one more than minimal fit");
    assert!(report.payload_chars_after <= ROOMY * LOW_WATERMARK_PCT / 100);
}

#[test]
fn the_savings_target_is_quantized_to_watermark_margins() {
    // limit 1,000 → watermark 750, margin 250.
    assert_eq!(target_savings_chars(1_000, 1_000), 250, "bracket floor");
    assert_eq!(target_savings_chars(1_001, 1_000), 500, "first crossing");
    assert_eq!(target_savings_chars(1_250, 1_000), 500, "bracket edge");
    assert_eq!(
        target_savings_chars(1_251, 1_000),
        750,
        "just past the edge"
    );
}

#[test]
fn a_payload_at_or_below_the_watermark_needs_no_savings() {
    assert_eq!(target_savings_chars(750, 1_000), 0);
    assert_eq!(target_savings_chars(0, 1_000), 0);
}

#[test]
fn a_degenerate_budget_clamps_the_margin_rather_than_dividing_by_zero() {
    // limit 0 → watermark 0, margin clamps to 1: the whole payload is owed.
    assert_eq!(target_savings_chars(42, 0), 42);
    // limit 1 → watermark 1 * 75 / 100 = 0, margin 1.
    assert_eq!(target_savings_chars(10, 1), 10);
}

/// The property this whole feature exists for: as a conversation grows past
/// the budget turn after turn, the elision set — and therefore the forwarded
/// prompt prefix llama.cpp matches its KV cache against — stays identical
/// until growth crosses a whole watermark margin, instead of shifting on
/// every turn.
#[test]
fn the_elision_set_is_stable_while_the_conversation_grows_within_a_bracket() {
    fn elided_indices(b: &Value) -> Vec<usize> {
        b["messages"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .filter(|(_, m)| m["content"] == TRUNCATION_PLACEHOLDER)
            .map(|(i, _)| i)
            .collect()
    }

    // Budget 200,000 → watermark 150,000, margin 50,000. Turn `t` resends the
    // full raw history as `t` identical 10k tool messages (the stage is
    // stateless), so the serialized payload is 35 + 10,029·t and each elision
    // saves 9,900. Growth per turn (10,029) exceeds savings per message
    // (9,900): the pre-watermark minimal-fit rule would have moved the elision
    // frontier on *every* triggered turn of this schedule.
    const LIMIT: usize = 200_000;

    let mut previous: Option<(Vec<usize>, Vec<Value>)> = None;
    let mut transitions = 0usize;

    for t in 1..=30 {
        let mut b = body(&vec![msg("tool", &big(10_000)); t]);
        let report = truncate_history(&mut b, LIMIT).unwrap();

        // Hand-computed: t ≤ 19 fits (190,586 ≤ 200,000 at t = 19); the
        // savings target is then 100,000 (turns 20-24, 11 elisions), 150,000
        // (turns 25-29, 16 elisions), and 200,000 (turn 30, 21 elisions).
        let expected: Vec<usize> = match t {
            ..=19 => vec![],
            20..=24 => (0..=10).collect(),
            25..=29 => (0..=15).collect(),
            _ => (0..=20).collect(),
        };
        let actual = elided_indices(&b);
        assert_eq!(actual, expected, "elision set at turn {t}");

        if report.messages_truncated > 0 {
            assert!(
                report.payload_chars_after <= LIMIT * LOW_WATERMARK_PCT / 100,
                "turn {t} must land at or below the watermark"
            );
        }

        let messages = b["messages"].as_array().unwrap().clone();
        if let Some((previous_set, previous_messages)) = previous {
            if actual == previous_set {
                // The KV-cache property: while the elision set holds, the
                // previous turn's entire forwarded message array is a literal
                // prefix of this turn's.
                assert_eq!(
                    &messages[..previous_messages.len()],
                    &previous_messages[..],
                    "prefix must be stable into turn {t}"
                );
            } else {
                transitions += 1;
            }
        }
        previous = Some((actual, messages));
    }

    // One initial truncation event (turn 20) plus two bracket crossings
    // (turns 25 and 30). Minimal-fit trimming would have made ~11 of the 30
    // turns move the frontier — one per triggered turn.
    assert_eq!(transitions, 3, "prefix breaks across the whole session");
}
