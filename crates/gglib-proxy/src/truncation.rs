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
//! 1. **Per-message threshold** — any unprotected `role: "tool"` or
//!    `role: "assistant"` message whose `content` string exceeds
//!    [`TOOL_CONTENT_THRESHOLD_CHARS`] has its content replaced with
//!    [`TRUNCATION_PLACEHOLDER`].
//!
//! 2. **Total budget hard abort** — if the payload still exceeds
//!    [`TOTAL_PAYLOAD_LIMIT_CHARS`] after step 1, the request is rejected
//!    with HTTP 400 / `context_length_exceeded` rather than forwarding a
//!    prompt that would cause the model to fail.
//!
//! 3. **Protected set** — `role: "system"` messages and the last
//!    [`PROTECTED_TAIL_COUNT`] messages by index (the immediate
//!    conversational context) are never modified.
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

/// Maximum total payload size in bytes.  If the serialised request body
/// exceeds this limit after per-message truncation, the proxy rejects the
/// request with HTTP 400 rather than forwarding an oversized prompt.
///
/// Approximation: 240,000 chars ÷ 4 ≈ 60,000 tokens.
pub const TOTAL_PAYLOAD_LIMIT_CHARS: usize = 240_000;

/// Number of trailing messages (by index) that are always preserved from
/// truncation regardless of role or content size.  These represent the
/// immediate conversational context the model needs to respond coherently.
pub const PROTECTED_TAIL_COUNT: usize = 4;

/// Replacement string inserted in place of truncated message content.
pub const TRUNCATION_PLACEHOLDER: &str =
    "[Raw tool output truncated by proxy to maintain context window. \
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
/// See the [module documentation](self) for the full algorithm.
///
/// # Returns
///
/// * `Ok((bytes, report))` — the (possibly mutated) body and a summary of
///   changes.  When no truncation was necessary the original `Bytes` value
///   is returned without re-serialisation.
/// * `Err(response)` — an HTTP 400 `context_length_exceeded` response when
///   the payload exceeds [`TOTAL_PAYLOAD_LIMIT_CHARS`] even after truncation.
pub fn truncate_history(body: Bytes) -> Result<(Bytes, TruncationReport), Response> {
    let payload_chars_before = body.len();

    // ── Pre-parse fast path ───────────────────────────────────────────────────
    // If the entire body is smaller than the per-message threshold no
    // individual message could possibly need truncation.
    if payload_chars_before <= TOOL_CONTENT_THRESHOLD_CHARS {
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

    // ── Post-parse fast path ──────────────────────────────────────────────────
    // Avoid mutation and re-serialisation when the payload is under the total
    // budget and no unprotected message has oversized string content.
    if payload_chars_before <= TOTAL_PAYLOAD_LIMIT_CHARS {
        let has_oversized_candidate = messages
            .iter()
            .enumerate()
            .filter(|(i, msg)| !is_tail_protected(*i, total) && is_truncation_candidate(msg))
            .any(|(_, msg)| exceeds_threshold(msg));

        if !has_oversized_candidate {
            return Ok((body, TruncationReport::zeroed(payload_chars_before)));
        }
    }

    // ── Mutate ────────────────────────────────────────────────────────────────
    let mut messages_truncated = 0usize;

    for i in 0..total {
        // Tail-protected messages and non-candidate roles (system, user) are
        // skipped entirely.
        if is_tail_protected(i, total) || !is_truncation_candidate(&messages[i]) {
            continue;
        }

        // Only string-form content is replaced.  Array-form content
        // (multi-part messages) is left untouched.  `tool_calls` is never
        // modified regardless of role.
        let should_truncate = messages[i]
            .get("content")
            .and_then(|c| c.as_str())
            .map(|s| s.len() > TOOL_CONTENT_THRESHOLD_CHARS)
            .unwrap_or(false);

        if should_truncate {
            messages[i]["content"] =
                serde_json::Value::String(TRUNCATION_PLACEHOLDER.to_string());
            messages_truncated += 1;
        }
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
    if payload_chars_after > TOTAL_PAYLOAD_LIMIT_CHARS {
        let response = (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::context_length_exceeded()),
        )
            .into_response();
        return Err(response);
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

/// Returns `true` if the message has string-form `content` that exceeds
/// [`TOOL_CONTENT_THRESHOLD_CHARS`].
#[inline]
fn exceeds_threshold(msg: &serde_json::Value) -> bool {
    msg.get("content")
        .and_then(|c| c.as_str())
        .map(|s| s.len() > TOOL_CONTENT_THRESHOLD_CHARS)
        .unwrap_or(false)
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

    // ── Fast path ─────────────────────────────────────────────────────────────

    #[test]
    fn fast_path_small_payload_returned_unchanged() {
        let body = make_body(vec![msg("user", "hello"), msg("assistant", "world")]);
        let original_len = body.len();
        let (out, report) = truncate_history(body).unwrap();
        assert_eq!(out.len(), original_len, "bytes must be identical on fast path");
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
        let (out, report) = truncate_history(body).unwrap();
        assert_eq!(out.len(), original_len);
        assert_eq!(report.messages_truncated, 0);
    }

    // ── Basic truncation ──────────────────────────────────────────────────────

    #[test]
    fn oversized_tool_content_replaced_with_placeholder() {
        let oversized = big(TOOL_CONTENT_THRESHOLD_CHARS + 1);
        let body = make_body(vec![
            msg("user", "run"),
            msg("tool", &oversized), // index 1 — not in tail (6 messages, tail = 2..5)
            msg("user", "a"),
            msg("user", "b"),
            msg("user", "c"),
            msg("user", "d"),
        ]);
        let (out, report) = truncate_history(body).unwrap();
        assert_eq!(report.messages_truncated, 1);
        let parsed: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(
            parsed["messages"][1]["content"],
            TRUNCATION_PLACEHOLDER,
            "oversized tool content must be replaced"
        );
        assert_eq!(
            parsed["messages"][0]["content"], "run",
            "other messages must be untouched"
        );
    }

    #[test]
    fn oversized_assistant_content_replaced() {
        let oversized = big(TOOL_CONTENT_THRESHOLD_CHARS + 1);
        let body = make_body(vec![
            msg("assistant", &oversized), // index 0, not in tail
            msg("user", "a"),
            msg("user", "b"),
            msg("user", "c"),
            msg("user", "d"),
            msg("user", "e"),
        ]);
        let (_, report) = truncate_history(body).unwrap();
        assert_eq!(report.messages_truncated, 1);
    }

    #[test]
    fn content_under_threshold_not_truncated() {
        let under = big(TOOL_CONTENT_THRESHOLD_CHARS - 1);
        let body = make_body(vec![
            msg("tool", &under), // under threshold — must not be replaced
            msg("user", "a"),
            msg("user", "b"),
            msg("user", "c"),
            msg("user", "d"),
            msg("user", "e"),
        ]);
        let (out, report) = truncate_history(body).unwrap();
        assert_eq!(report.messages_truncated, 0);
        let parsed: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(parsed["messages"][0]["content"].as_str().unwrap().len(), under.len());
    }

    // ── Protection ────────────────────────────────────────────────────────────

    #[test]
    fn system_message_never_truncated() {
        let oversized = big(TOOL_CONTENT_THRESHOLD_CHARS + 1);
        // System message at index 0 with oversized content — must never be touched.
        let body = make_body(vec![
            serde_json::json!({"role": "system", "content": oversized.clone()}),
            msg("user", "a"),
            msg("user", "b"),
            msg("user", "c"),
            msg("user", "d"),
            msg("user", "e"),
        ]);
        let (out, report) = truncate_history(body).unwrap();
        assert_eq!(report.messages_truncated, 0);
        let parsed: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(
            parsed["messages"][0]["content"].as_str().unwrap().len(),
            oversized.len(),
            "system message content must be preserved"
        );
    }

    #[test]
    fn tail_messages_not_truncated() {
        let oversized = big(TOOL_CONTENT_THRESHOLD_CHARS + 1);
        // Tool message sits entirely within the protected tail (≤ PROTECTED_TAIL_COUNT
        // total messages → tail_start = 0 → every message is protected).
        let body = make_body(vec![
            msg("user", "hi"),
            msg("tool", &oversized),
            msg("assistant", "ok"),
            msg("user", "thanks"),
        ]);
        let (_, report) = truncate_history(body).unwrap();
        assert_eq!(
            report.messages_truncated, 0,
            "messages inside the protected tail must not be truncated"
        );
    }

    #[test]
    fn non_tail_tool_truncated_tail_tool_preserved() {
        let oversized = big(TOOL_CONTENT_THRESHOLD_CHARS + 1);
        // 7 messages → tail starts at index 3.
        // Index 0 tool: NOT in tail → truncated.
        // Index 3 tool: in tail → preserved.
        let body = make_body(vec![
            msg("tool", &oversized),   // index 0 — truncated
            msg("user", "mid"),
            msg("user", "mid2"),
            msg("tool", &oversized),   // index 3 — tail-protected
            msg("user", "a"),
            msg("user", "b"),
            msg("user", "c"),
        ]);
        let (out, report) = truncate_history(body).unwrap();
        assert_eq!(report.messages_truncated, 1, "only the non-tail tool must be truncated");
        let parsed: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(parsed["messages"][0]["content"], TRUNCATION_PLACEHOLDER);
        assert_eq!(
            parsed["messages"][3]["content"].as_str().unwrap().len(),
            oversized.len(),
            "tail tool content must be preserved"
        );
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
        let result = truncate_history(body);
        assert!(
            result.is_err(),
            "must return Err when over budget even after truncation"
        );
    }

    // ── Special content forms ─────────────────────────────────────────────────

    #[test]
    fn array_form_content_skipped() {
        let oversized_str = big(TOOL_CONTENT_THRESHOLD_CHARS + 1);
        // Index 0: string tool (forces mutation path by triggering truncation)
        // Index 1: array-form tool (must not be truncated)
        let body = Bytes::from(
            serde_json::to_vec(&serde_json::json!({
                "model": "test",
                "messages": [
                    {"role": "tool", "tool_call_id": "c1", "content": oversized_str},
                    {"role": "tool", "tool_call_id": "c2", "content": [{"type": "text", "text": "big array content"}]},
                    {"role": "user", "content": "a"},
                    {"role": "user", "content": "b"},
                    {"role": "user", "content": "c"},
                    {"role": "user", "content": "d"},
                ]
            }))
            .unwrap(),
        );
        let (out, report) = truncate_history(body).unwrap();
        assert_eq!(report.messages_truncated, 1, "only the string-form tool should be counted");
        let parsed: serde_json::Value = serde_json::from_slice(&out).unwrap();
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
        let (_, report) = truncate_history(body).unwrap();
        assert_eq!(report.messages_truncated, 0);
    }

    #[test]
    fn tool_calls_preserved_when_content_truncated() {
        // An assistant message that has BOTH oversized content AND tool_calls.
        // Content must be replaced; tool_calls must survive.
        let oversized = big(TOOL_CONTENT_THRESHOLD_CHARS + 1);
        let body = Bytes::from(
            serde_json::to_vec(&serde_json::json!({
                "model": "test",
                "messages": [
                    {
                        "role": "assistant",
                        "content": oversized,
                        "tool_calls": [{"id": "call_1", "type": "function", "function": {"name": "foo", "arguments": "{}"}}]
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
        let (out, report) = truncate_history(body).unwrap();
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
        let (out, report) = truncate_history(bad.clone()).unwrap();
        assert_eq!(out, bad, "non-JSON body must be returned byte-for-byte");
        assert_eq!(report.messages_truncated, 0);
        assert_eq!(report.payload_chars_before, bad.len());
    }

    #[test]
    fn missing_messages_field_passes_through_unchanged() {
        let body = Bytes::from(b"{\"model\":\"test\"}" as &[u8]);
        let original_len = body.len();
        let (out, report) = truncate_history(body).unwrap();
        assert_eq!(out.len(), original_len);
        assert_eq!(report.messages_truncated, 0);
    }

    // ── Report fields ─────────────────────────────────────────────────────────

    #[test]
    fn report_chars_after_less_than_before_when_truncated() {
        let oversized = big(TOOL_CONTENT_THRESHOLD_CHARS + 1);
        let body = make_body(vec![
            msg("tool", &oversized),
            msg("user", "a"),
            msg("user", "b"),
            msg("user", "c"),
            msg("user", "d"),
            msg("user", "e"),
        ]);
        let (_, report) = truncate_history(body).unwrap();
        assert!(
            report.payload_chars_after < report.payload_chars_before,
            "after must be smaller than before when content was replaced"
        );
    }
}
