//! History truncation pass for the proxy request pipeline.
//!
//! ## Problem
//!
//! Client-side context compaction can be broken for custom OpenAI-compatible
//! endpoints.  Each tool call response is permanently embedded in the chat
//! history by the client, causing the prompt to balloon past the local
//! model's context window after several tool-heavy turns.  The model then
//! falls into repetition or logic loops.
//!
//! ## Defence
//!
//! [`truncate_history`] is a stateless `Bytes → Result<(Bytes, TruncationReport), Response>`
//! pass applied to every inbound `/v1/chat/completions` request before it
//! reaches the upstream inference engine:
//!
//! 1. **Budget gate** — if the whole payload already fits within the
//!    request's character budget (the `limit_chars` argument, floored at
//!    [`TOTAL_PAYLOAD_LIMIT_CHARS`]) the body is forwarded **unchanged**. No
//!    history is elided while there is room, so the model keeps maximum
//!    context on every turn that does not actually need trimming.
//!
//! 2. **Oldest-first trim** — only when the payload exceeds the budget are
//!    messages elided, and then only as many as necessary: unprotected
//!    `role: "tool"` / `role: "assistant"` messages whose `content` string
//!    exceeds [`TOOL_CONTENT_THRESHOLD_CHARS`] are replaced with
//!    [`TRUNCATION_PLACEHOLDER`] **from oldest to newest**, stopping as soon
//!    as the running payload estimate drops back under budget. The freshest
//!    tool outputs — the ones the model most likely still needs — are the
//!    last to be sacrificed.
//!
//! 3. **Total budget hard abort** — if the payload still exceeds the budget
//!    after every eligible message has been trimmed (e.g. an enormous
//!    protected system prompt), the request is rejected with HTTP 400 /
//!    `context_length_exceeded` rather than forwarding a prompt that would
//!    cause the model to fail.
//!
//! 4. **Protected set** — `role: "system"` messages and the last
//!    [`PROTECTED_TAIL_COUNT`] messages by index (the immediate
//!    conversational context, spanning several recent tool-call/result
//!    pairs) are never modified.
//!
//! ## Zero blast radius
//!
//! On JSON parse failure the original `Bytes` are returned unchanged so the
//! upstream can produce its own diagnostic error.

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use bytes::Bytes;

use crate::models::ErrorResponse;

// =============================================================================
// Constants
// =============================================================================

/// Maximum number of characters allowed in a single unprotected `role: "tool"`
/// or `role: "assistant"` message `content` string before it is replaced with
/// [`TRUNCATION_PLACEHOLDER`].
pub const TOOL_CONTENT_THRESHOLD_CHARS: usize = 2_000;

/// Default (and floor) total payload size in bytes. Callers of
/// [`truncate_history`] supply a per-request character budget derived from
/// the running model's live context size; this constant is the fallback
/// when no model is running and the floor below which a caller-supplied
/// budget is never allowed to shrink, so no request ever gets less headroom
/// than the historical default.
///
/// Approximation: 240,000 chars ÷ 4 ≈ 60,000 tokens.
pub const TOTAL_PAYLOAD_LIMIT_CHARS: usize = 240_000;

/// Character-to-token conversion factor used solely to translate
/// [`TOTAL_PAYLOAD_LIMIT_CHARS`] (a **character** budget) into a **token**
/// count for [`TOTAL_PAYLOAD_LIMIT_TOKENS`].
///
/// This value is not an attempt at precise real-world tokenization — it is
/// deliberately chosen to match the GitHub Copilot LLM Gateway extension's
/// own `TOKEN_CONSTANTS.CHARS_PER_TOKEN = 4` (see its `tokenBudget.ts`), so
/// that gglib's advertised `context_window` (see `crate::models::ModelInfo`)
/// and the extension's own char→token budget estimate agree on the same
/// conversion factor. What matters here is consistency between the two
/// sides, not tokenizer accuracy.
pub const CHARS_PER_TOKEN_APPROX: usize = 4;

/// [`TOTAL_PAYLOAD_LIMIT_CHARS`] expressed as a **token** count.
///
/// The token-denominated floor of the proxy's payload guard: requests up to
/// this size are always accepted regardless of serving context (see the
/// `limit_chars` floor in [`truncate_history`]). `/v1/models` advertisement
/// no longer uses this value — models advertise the context they would
/// actually be served with (see `server::list_models`).
pub const TOTAL_PAYLOAD_LIMIT_TOKENS: usize = TOTAL_PAYLOAD_LIMIT_CHARS / CHARS_PER_TOKEN_APPROX;

/// Number of trailing messages (by index) that are always preserved from
/// truncation regardless of role or content size.  These represent the
/// immediate conversational context the model needs to respond coherently —
/// sized to span several recent tool-call/result pairs so a live tool
/// exchange is never half-elided.
pub const PROTECTED_TAIL_COUNT: usize = 8;

/// Replacement string inserted in place of truncated message content.
pub const TRUNCATION_PLACEHOLDER: &str = "[Raw tool output truncated by proxy to maintain context window. \
     Rely on your previous observations.]";

// =============================================================================
// Report
// =============================================================================

/// Summary of what [`truncate_history`] did to a request body.
///
/// Returned alongside the (possibly modified) body on the `Ok` path so
/// callers can record observability metrics without re-computing the same
/// values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TruncationReport {
    /// Approximate payload size in bytes before truncation.
    pub payload_chars_before: usize,
    /// Approximate payload size in bytes after truncation.  Equal to
    /// `payload_chars_before` when no changes were made (fast path).
    pub payload_chars_after: usize,
    /// Number of messages whose `content` was replaced with
    /// [`TRUNCATION_PLACEHOLDER`].
    pub messages_truncated: usize,
    /// `true` when the hard-abort budget check triggered.  When the hard abort
    /// fires the function returns `Err(response)` rather than `Ok(...)`, so
    /// this field is always `false` on the `Ok` path.  It exists to satisfy
    /// the `ContextSnapshot` observability record that the caller constructs
    /// from this report for the `/v1/proxy/status` endpoint.
    pub was_clamped: bool,
}

impl TruncationReport {
    /// A no-op report for the fast path and error-passthrough paths.
    fn zeroed(payload_chars_before: usize) -> Self {
        Self {
            payload_chars_before,
            payload_chars_after: payload_chars_before,
            messages_truncated: 0,
            was_clamped: false,
        }
    }
}

// =============================================================================
// Core function
// =============================================================================

/// Apply history truncation to a raw `/v1/chat/completions` request body.
///
/// `limit_chars` is the total payload character budget for this request,
/// typically derived from the running model's live context size
/// (`effective_ctx × CHARS_PER_TOKEN_APPROX`). It is floored at
/// [`TOTAL_PAYLOAD_LIMIT_CHARS`], so a caller can never shrink the budget
/// below the historical default.
///
/// See the [module documentation](self) for the full algorithm.
///
/// # Returns
///
/// * `Ok((bytes, report))` — the (possibly mutated) body and a summary of
///   changes.  When no truncation was necessary the original `Bytes` value
///   is returned without re-serialisation.
/// * `Err(response)` — an HTTP 400 `context_length_exceeded` response when
///   the payload exceeds the character budget even after truncation.
pub fn truncate_history(
    body: Bytes,
    limit_chars: usize,
) -> Result<(Bytes, TruncationReport), Box<Response>> {
    let limit_chars = limit_chars.max(TOTAL_PAYLOAD_LIMIT_CHARS);
    let payload_chars_before = body.len();

    // ── Budget gate ───────────────────────────────────────────────────────────
    // While the whole payload fits within budget there is nothing to do:
    // forward it byte-for-byte and keep the model's full history intact. This
    // is the common case and the reason truncation no longer mutilates history
    // pre-emptively.
    if payload_chars_before <= limit_chars {
        return Ok((body, TruncationReport::zeroed(payload_chars_before)));
    }

    // ── Parse ─────────────────────────────────────────────────────────────────
    let mut value: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => {
            // Zero blast radius: non-JSON bodies pass through unchanged.
            return Ok((body, TruncationReport::zeroed(payload_chars_before)));
        }
    };

    let Some(messages) = value.get_mut("messages").and_then(|v| v.as_array_mut()) else {
        // No messages array — nothing to truncate.
        return Ok((body, TruncationReport::zeroed(payload_chars_before)));
    };

    let total = messages.len();
    let placeholder_len = TRUNCATION_PLACEHOLDER.len();

    // ── Oldest-first trim ──────────────────────────────────────────────────────
    // Walk from the oldest message toward the newest, eliding eligible
    // oversized content only until the running payload estimate drops back
    // under budget. `running` tracks the approximate payload size as each
    // replacement shrinks it, so we stop at the minimum necessary and leave
    // the freshest tool outputs intact.
    let mut messages_truncated = 0usize;
    let mut running = payload_chars_before;

    for (i, msg) in messages.iter_mut().enumerate() {
        if running <= limit_chars {
            break;
        }
        // Tail-protected messages and non-candidate roles (system, user) are
        // skipped entirely.
        if is_tail_protected(i, total) || !is_truncation_candidate(msg) {
            continue;
        }

        // Only string-form content is replaced.  Array-form content
        // (multi-part messages) is left untouched.  `tool_calls` is never
        // modified regardless of role.
        let Some(content_len) = msg
            .get("content")
            .and_then(|c| c.as_str())
            .map(str::len)
            .filter(|len| *len > TOOL_CONTENT_THRESHOLD_CHARS)
        else {
            continue;
        };

        msg["content"] = serde_json::Value::String(TRUNCATION_PLACEHOLDER.to_string());
        messages_truncated += 1;
        // Each replacement reclaims (content_len - placeholder_len) chars.
        running = running.saturating_sub(content_len.saturating_sub(placeholder_len));
    }

    // ── Re-serialise ──────────────────────────────────────────────────────────
    let new_bytes = match serde_json::to_vec(&value) {
        Ok(v) => Bytes::from(v),
        Err(_) => {
            // Serialisation failed — return original body unchanged.
            return Ok((body, TruncationReport::zeroed(payload_chars_before)));
        }
    };

    let payload_chars_after = new_bytes.len();

    // ── Budget check ──────────────────────────────────────────────────────────
    // Hard abort if still over budget (e.g. a huge protected system prompt).
    if payload_chars_after > limit_chars {
        let response = (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::context_length_exceeded()),
        )
            .into_response();
        return Err(Box::new(response));
    }

    Ok((
        new_bytes,
        TruncationReport {
            payload_chars_before,
            payload_chars_after,
            messages_truncated,
            was_clamped: false,
        },
    ))
}

// =============================================================================
// Helpers
// =============================================================================

/// Returns `true` if the message at `index` (in a list of `total` messages)
/// falls within the protected tail window and must not be truncated.
#[inline]
fn is_tail_protected(index: usize, total: usize) -> bool {
    index >= total.saturating_sub(PROTECTED_TAIL_COUNT)
}

/// Returns `true` if this message role is eligible for content truncation.
///
/// Only `role: "tool"` and `role: "assistant"` are candidates.
/// `role: "system"` and `role: "user"` are never truncated.
#[inline]
fn is_truncation_candidate(msg: &serde_json::Value) -> bool {
    matches!(
        msg.get("role").and_then(|r| r.as_str()).unwrap_or(""),
        "tool" | "assistant"
    )
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ── Builders ─────────────────────────────────────────────────────────────

    fn make_body(messages: Vec<serde_json::Value>) -> Bytes {
        Bytes::from(
            serde_json::to_vec(&serde_json::json!({
                "model": "test-model",
                "messages": messages,
            }))
            .unwrap(),
        )
    }

    fn msg(role: &str, content: &str) -> serde_json::Value {
        serde_json::json!({"role": role, "content": content})
    }

    fn big(n: usize) -> String {
        "x".repeat(n)
    }

    /// Test shorthand: call [`truncate_history`] with the default budget.
    fn truncate_default(body: Bytes) -> Result<(Bytes, TruncationReport), Box<Response>> {
        truncate_history(body, TOTAL_PAYLOAD_LIMIT_CHARS)
    }

    // ── Fast path ──────────────────────────────────────────────────────────────────

    #[test]
    fn fast_path_small_payload_returned_unchanged() {
        let body = make_body(vec![msg("user", "hello"), msg("assistant", "world")]);
        let original_len = body.len();
        let (out, report) = truncate_default(body).unwrap();
        assert_eq!(
            out.len(),
            original_len,
            "bytes must be identical on fast path"
        );
        assert_eq!(report.messages_truncated, 0);
        assert_eq!(report.payload_chars_before, report.payload_chars_after);
        assert!(!report.was_clamped);
    }

    #[test]
    fn fast_path_under_budget_no_candidates_unchanged() {
        // Payload under budget, tool message content is under threshold.
        let body = make_body(vec![
            msg("user", "run tool"),
            msg("tool", "small result"),
            msg("user", "a"),
            msg("user", "b"),
            msg("user", "c"),
            msg("user", "d"),
        ]);
        let original_len = body.len();
        let (out, report) = truncate_default(body).unwrap();
        assert_eq!(out.len(), original_len);
        assert_eq!(report.messages_truncated, 0);
    }

    // ── Budget-gated, oldest-first truncation ─────────────────────────────────

    /// Build a chat body with `leading_big` oversized tool messages (oldest,
    /// outside the protected tail) followed by `tail` small user messages.
    fn over_budget_body(leading_big: usize, big_chars: usize, tail: usize) -> Bytes {
        let mut messages = Vec::new();
        for _ in 0..leading_big {
            messages.push(msg("tool", &big(big_chars)));
        }
        for _ in 0..tail {
            messages.push(msg("user", "ok"));
        }
        make_body(messages)
    }

    #[test]
    fn under_budget_leaves_oversized_content_intact() {
        // Behavioural guard: with room in the budget, even a giant tool output
        // is forwarded untouched — history is no longer mutilated pre-emptively.
        let oversized = big(TOOL_CONTENT_THRESHOLD_CHARS * 10);
        let body = make_body(vec![
            msg("tool", &oversized),
            msg("user", "a"),
            msg("user", "b"),
            msg("user", "c"),
            msg("user", "d"),
        ]);
        let original_len = body.len();
        let (out, report) = truncate_default(body).unwrap();
        assert_eq!(report.messages_truncated, 0, "nothing truncated under budget");
        assert_eq!(out.len(), original_len, "body forwarded byte-for-byte");
    }

    #[test]
    fn over_budget_trims_oldest_first_and_stops_early() {
        // 4 large tool messages (indices 0..3, outside the 8-message tail) plus
        // 8 small tail messages → total 12, tail_start = 4. Each big message is
        // 100k chars, so the payload (~400k) exceeds the 240k budget. Trimming
        // the two oldest brings it back under budget; the two newer big
        // messages must survive.
        let body = over_budget_body(4, 100_000, 8);
        let (out, report) = truncate_history(body, TOTAL_PAYLOAD_LIMIT_CHARS).unwrap();

        let parsed: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(
            report.messages_truncated, 2,
            "only as many oldest messages as needed to get under budget"
        );
        assert_eq!(
            parsed["messages"][0]["content"], TRUNCATION_PLACEHOLDER,
            "oldest big message trimmed"
        );
        assert_eq!(
            parsed["messages"][1]["content"], TRUNCATION_PLACEHOLDER,
            "second-oldest big message trimmed"
        );
        assert_eq!(
            parsed["messages"][2]["content"].as_str().unwrap().len(),
            100_000,
            "newer big message preserved once under budget"
        );
        assert_eq!(
            parsed["messages"][3]["content"].as_str().unwrap().len(),
            100_000,
            "newest unprotected big message preserved"
        );
        assert!(report.payload_chars_after <= TOTAL_PAYLOAD_LIMIT_CHARS);
    }

    #[test]
    fn over_budget_trims_assistant_content_too() {
        // Same shape but the oversized messages are assistant turns.
        let mut messages = vec![
            msg("assistant", &big(150_000)),
            msg("assistant", &big(150_000)),
        ];
        for _ in 0..8 {
            messages.push(msg("user", "ok"));
        }
        let body = make_body(messages);
        let (_, report) = truncate_history(body, TOTAL_PAYLOAD_LIMIT_CHARS).unwrap();
        assert!(
            report.messages_truncated >= 1,
            "assistant content is an eligible truncation candidate"
        );
    }

    #[test]
    fn content_under_threshold_not_truncated_even_over_budget() {
        // A payload made entirely of many small tool messages exceeds budget by
        // sheer count, but none individually exceeds the per-message threshold,
        // so none is eligible — the request hard-aborts rather than trimming
        // sub-threshold content.
        let mut messages = Vec::new();
        for _ in 0..300 {
            messages.push(msg("tool", &big(TOOL_CONTENT_THRESHOLD_CHARS - 1)));
        }
        let body = make_body(messages);
        let result = truncate_history(body, TOTAL_PAYLOAD_LIMIT_CHARS);
        assert!(
            result.is_err(),
            "no eligible (over-threshold) content to trim → hard abort"
        );
    }

    // ── Protection ────────────────────────────────────────────────────────────

    #[test]
    fn system_message_never_truncated_even_over_budget() {
        // A huge system prompt plus enough oversized tool messages to blow the
        // budget: the tools are trimmed, the system message is untouched.
        let big_system = big(120_000);
        let mut messages = vec![
            serde_json::json!({"role": "system", "content": big_system.clone()}),
            msg("tool", &big(120_000)),
            msg("tool", &big(120_000)),
        ];
        for _ in 0..8 {
            messages.push(msg("user", "ok"));
        }
        let body = make_body(messages);
        let (out, _report) = truncate_history(body, TOTAL_PAYLOAD_LIMIT_CHARS).unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(
            parsed["messages"][0]["content"].as_str().unwrap().len(),
            big_system.len(),
            "system message content must be preserved"
        );
    }

    #[test]
    fn protected_tail_preserved_over_budget() {
        // A single huge unprotected tool at index 0 (total = 9, tail_start = 1)
        // plus two moderate oversized tools inside the protected tail. Trimming
        // the index-0 message alone drops the payload under budget, so the tail
        // tools must survive untouched.
        let mut messages = vec![msg("tool", &big(300_000))];
        messages.push(msg("tool", &big(30_000)));
        messages.push(msg("tool", &big(30_000)));
        for _ in 0..6 {
            messages.push(msg("user", "ok"));
        }
        let body = make_body(messages);
        let (out, report) = truncate_history(body, TOTAL_PAYLOAD_LIMIT_CHARS).unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(
            parsed["messages"][0]["content"], TRUNCATION_PLACEHOLDER,
            "the single unprotected tool must be trimmed"
        );
        assert_eq!(
            parsed["messages"][1]["content"].as_str().unwrap().len(),
            30_000,
            "tail tool #1 must be preserved"
        );
        assert_eq!(
            parsed["messages"][2]["content"].as_str().unwrap().len(),
            30_000,
            "tail tool #2 must be preserved"
        );
        assert_eq!(report.messages_truncated, 1);
    }

    // ── Hard abort ────────────────────────────────────────────────────────────

    #[test]
    fn over_budget_after_truncation_returns_400() {
        // A system prompt so large that the payload stays over budget even
        // after all non-protected messages are truncated.
        let huge_system = big(TOTAL_PAYLOAD_LIMIT_CHARS);
        let body = make_body(vec![
            serde_json::json!({"role": "system", "content": huge_system}),
            msg("tool", &big(TOOL_CONTENT_THRESHOLD_CHARS + 1)),
            msg("user", "tail"),
            msg("user", "tail"),
            msg("user", "tail"),
            msg("user", "tail"),
        ]);
        let result = truncate_default(body);
        assert!(
            result.is_err(),
            "must return Err when over budget even after truncation"
        );
    }

    // ── Special content forms ─────────────────────────────────────────────────

    #[test]
    fn array_form_content_skipped() {
        // Index 0: oversized string tool (eligible, will be trimmed once over
        // budget). Index 1: array-form tool (must never be truncated). Padded
        // with a protected tail; total payload exceeds budget.
        let mut messages = vec![
            serde_json::json!({"role": "tool", "tool_call_id": "c1", "content": big(250_000)}),
            serde_json::json!({"role": "tool", "tool_call_id": "c2", "content": [{"type": "text", "text": "big array content"}]}),
        ];
        for _ in 0..8 {
            messages.push(msg("user", "ok"));
        }
        let body = make_body(messages);
        let (out, report) = truncate_history(body, TOTAL_PAYLOAD_LIMIT_CHARS).unwrap();
        assert_eq!(
            report.messages_truncated, 1,
            "only the string-form tool should be counted"
        );
        let parsed: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(parsed["messages"][0]["content"], TRUNCATION_PLACEHOLDER);
        assert!(
            parsed["messages"][1]["content"].is_array(),
            "array-form content must be left as an array"
        );
    }

    #[test]
    fn assistant_without_content_not_counted() {
        // An assistant message that has tool_calls but no content field must
        // not be modified or counted as truncated.
        let body = Bytes::from(
            serde_json::to_vec(&serde_json::json!({
                "model": "test",
                "messages": [
                    {
                        "role": "assistant",
                        "tool_calls": [{"id": "c1", "type": "function", "function": {"name": "f", "arguments": "{}"}}]
                    },
                    {"role": "user", "content": "a"},
                    {"role": "user", "content": "b"},
                    {"role": "user", "content": "c"},
                    {"role": "user", "content": "d"},
                    {"role": "user", "content": "e"},
                ]
            }))
            .unwrap(),
        );
        let (_, report) = truncate_default(body).unwrap();
        assert_eq!(report.messages_truncated, 0);
    }

    #[test]
    fn tool_calls_preserved_when_content_truncated() {
        // An assistant message with BOTH oversized content AND tool_calls, old
        // enough to be trimmed once the payload is over budget. Content must be
        // replaced; tool_calls must survive.
        let mut messages = vec![serde_json::json!({
            "role": "assistant",
            "content": big(250_000),
            "tool_calls": [{"id": "call_1", "type": "function", "function": {"name": "foo", "arguments": "{}"}}]
        })];
        for _ in 0..8 {
            messages.push(msg("user", "ok"));
        }
        let body = make_body(messages);
        let (out, report) = truncate_history(body, TOTAL_PAYLOAD_LIMIT_CHARS).unwrap();
        assert_eq!(report.messages_truncated, 1);
        let parsed: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(parsed["messages"][0]["content"], TRUNCATION_PLACEHOLDER);
        assert_eq!(
            parsed["messages"][0]["tool_calls"][0]["id"], "call_1",
            "tool_calls must be preserved after content truncation"
        );
    }

    // ── Zero blast radius ─────────────────────────────────────────────────────

    #[test]
    fn malformed_json_passes_through_unchanged() {
        let bad = Bytes::from(b"not json {{ garbage }}" as &[u8]);
        let (out, report) = truncate_default(bad.clone()).unwrap();
        assert_eq!(out, bad, "non-JSON body must be returned byte-for-byte");
        assert_eq!(report.messages_truncated, 0);
        assert_eq!(report.payload_chars_before, bad.len());
    }

    #[test]
    fn missing_messages_field_passes_through_unchanged() {
        let body = Bytes::from(b"{\"model\":\"test\"}" as &[u8]);
        let original_len = body.len();
        let (out, report) = truncate_default(body).unwrap();
        assert_eq!(out.len(), original_len);
        assert_eq!(report.messages_truncated, 0);
    }

    // ── Report fields ─────────────────────────────────────────────────────────

    #[test]
    fn report_chars_after_less_than_before_when_truncated() {
        let body = over_budget_body(4, 100_000, 8);
        let (_, report) = truncate_history(body, TOTAL_PAYLOAD_LIMIT_CHARS).unwrap();
        assert!(
            report.payload_chars_after < report.payload_chars_before,
            "after must be smaller than before when content was replaced"
        );
    }

    // ── Dynamic limit ────────────────────────────────────────────────────────────────

    #[test]
    fn dynamic_limit_allows_payload_above_default_floor() {
        // A protected system prompt bigger than the historical floor but
        // within a raised (e.g. 131k-ctx-derived) budget must pass.
        let raised_limit = TOTAL_PAYLOAD_LIMIT_CHARS * 2;
        let big_system = big(TOTAL_PAYLOAD_LIMIT_CHARS + 10_000);
        let body = make_body(vec![
            serde_json::json!({"role": "system", "content": big_system}),
            msg("user", "a"),
            msg("user", "b"),
            msg("user", "c"),
            msg("user", "d"),
        ]);
        let result = truncate_history(body, raised_limit);
        assert!(
            result.is_ok(),
            "payload above the floor but within the dynamic budget must pass"
        );
    }

    #[test]
    fn dynamic_limit_hard_aborts_above_raised_budget() {
        // Even a raised budget must still hard-abort payloads that exceed it.
        let raised_limit = TOTAL_PAYLOAD_LIMIT_CHARS * 2;
        let big_system = big(raised_limit + 10_000);
        let body = make_body(vec![
            serde_json::json!({"role": "system", "content": big_system}),
            msg("user", "a"),
            msg("user", "b"),
            msg("user", "c"),
            msg("user", "d"),
        ]);
        let result = truncate_history(body, raised_limit);
        assert!(
            result.is_err(),
            "payload above the dynamic budget must still hard-abort"
        );
    }

    #[test]
    fn dynamic_limit_never_shrinks_below_floor() {
        // A tiny caller-supplied limit must be floored at the default: a
        // payload under the floor passes even when limit_chars is tiny.
        let big_system = big(TOTAL_PAYLOAD_LIMIT_CHARS - 50_000);
        let body = make_body(vec![
            serde_json::json!({"role": "system", "content": big_system}),
            msg("user", "a"),
            msg("user", "b"),
            msg("user", "c"),
            msg("user", "d"),
        ]);
        let result = truncate_history(body, 1_000);
        assert!(
            result.is_ok(),
            "limit_chars below the floor must be raised to the floor, not enforced"
        );
    }
}
