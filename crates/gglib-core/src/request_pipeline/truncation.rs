//! Stage 3: history truncation.
//!
//! ## Problem
//!
//! Client-side context compaction can be broken for custom `OpenAI`-compatible
//! endpoints. Each tool-call result is permanently embedded in the chat history
//! by the client, so the prompt balloons past the model's context window after
//! several tool-heavy turns and the model falls into repetition or logic loops.
//!
//! ## Defence
//!
//! [`truncate_history`] is a stateless pass over the request body:
//!
//! 1. **Budget gate** — if the serialized payload already fits within
//!    `limit_chars` the body is left **completely untouched**. No history is
//!    elided while there is room, so the model keeps maximum context on every
//!    turn that does not actually need trimming.
//!
//! 2. **Oldest-first trim to a low watermark** — only when the payload exceeds
//!    the budget are messages elided: unprotected `role: "tool"` /
//!    `role: "assistant"` messages whose `content` string exceeds
//!    [`TOOL_CONTENT_THRESHOLD_CHARS`] are replaced with
//!    [`TRUNCATION_PLACEHOLDER`] **from oldest to newest**, until the estimated
//!    savings reach a quantized target aimed at [`LOW_WATERMARK_PCT`] of the
//!    budget. The freshest tool outputs — the ones the model most likely still
//!    needs — are the last to be sacrificed.
//!
//! 3. **Hard abort** — if the payload still exceeds the budget after every
//!    eligible message has been trimmed (an enormous protected system prompt,
//!    say), [`TruncationError`] is returned rather than forwarding a prompt
//!    that would fail at the model. Each surface maps that to its own idiom.
//!
//! 4. **Protected set** — `role: "system"` messages and the last
//!    [`PROTECTED_TAIL_COUNT`] messages by index (the immediate conversational
//!    context, spanning several recent tool-call/result pairs) are never
//!    modified. Neither is `tool_calls`, at any role.
//!
//! ## Why trim past the budget, and why the target is quantized
//!
//! This stage is stateless and clients resend the full raw history every turn,
//! so an elision never persists: each request re-derives the elision set from
//! scratch. Trimming to just under the budget therefore moves the elision
//! frontier forward by ~one message on every turn of a long agentic session —
//! and every move shifts the first byte at which the forwarded prompt differs
//! from the previous turn's, breaking llama.cpp's common-prefix KV-cache match
//! and forcing a near-full prompt re-prefill each turn.
//!
//! Aiming lower (a classic low watermark) is not enough by itself: a minimal
//! elision set computed against *any* fixed threshold still grows at the same
//! per-turn cadence. Instead the savings target is quantized to whole
//! multiples of the watermark margin (budget − watermark, 25% of budget).
//! Within one margin's worth of payload growth the target — and therefore the
//! elision set — is identical across turns, so consecutive requests share
//! their prompt prefix and llama-server only prefills the new tail. When
//! growth crosses a margin boundary, several messages are elided at once and
//! the cycle restarts: the post-trim payload lands at or below the watermark,
//! sawtoothing within (50%, 75%] of budget, and prefix breaks happen once per
//! ~25%-of-budget of growth instead of once per turn.
//!
//! ## The budget is the model's, and only the model's
//!
//! `limit_chars` is a **character** budget, derived from the model's context
//! size in tokens via [`CHARS_PER_TOKEN_APPROX`]. There is no floor: a
//! 4096-token model gets a ~16,000-character budget and a 262,144-token model
//! gets a ~1,000,000-character one. Callers that know the *live* serving
//! context and a better chars-per-token ratio (the proxy learns one per model
//! from observed usage frames) pass their own number;
//! [`ModelContext::context_budget_chars`] is the answer for everyone else.
//!
//! [`ModelContext::context_budget_chars`]: super::ModelContext::context_budget_chars

use serde_json::Value;

// =============================================================================
// Constants
// =============================================================================

/// Maximum number of characters allowed in a single unprotected `role: "tool"`
/// or `role: "assistant"` message `content` string before it is eligible for
/// replacement with [`TRUNCATION_PLACEHOLDER`].
pub const TOOL_CONTENT_THRESHOLD_CHARS: usize = 2_000;

/// Character-to-token conversion factor used to translate a model's **token**
/// context size into the **character** budget [`truncate_history`] measures.
///
/// This is not an attempt at precise real-world tokenization — it deliberately
/// matches the GitHub Copilot LLM Gateway extension's own
/// `TOKEN_CONSTANTS.CHARS_PER_TOKEN = 4` (see its `tokenBudget.ts`), so that
/// gglib's advertised context window and the extension's own char-to-token
/// budget estimate agree on the same conversion factor. What matters is
/// consistency between the two sides, not tokenizer accuracy.
pub const CHARS_PER_TOKEN_APPROX: usize = 4;

/// Number of trailing messages (by index) always preserved from truncation
/// regardless of role or content size.
///
/// These are the immediate conversational context the model needs to respond
/// coherently — sized to span several recent tool-call/result pairs so a live
/// tool exchange is never half-elided.
pub const PROTECTED_TAIL_COUNT: usize = 8;

/// Low watermark the trim aims for, as a percentage of the request budget.
///
/// Once truncation is triggered (payload over `limit_chars`), the stage trims
/// past the budget down toward this fraction of it, buying several turns of
/// headroom in which follow-up requests need no new elisions — the property
/// that keeps the forwarded prompt prefix stable for llama.cpp's KV cache.
/// See the module docs for why the savings target is also quantized.
pub const LOW_WATERMARK_PCT: usize = 75;

/// Replacement string inserted in place of truncated message content.
pub const TRUNCATION_PLACEHOLDER: &str = "[Raw tool output truncated by proxy to maintain context window. \
     Rely on your previous observations.]";

// =============================================================================
// Report and error
// =============================================================================

/// Summary of what [`truncate_history`] did to a request body.
///
/// Callers record observability metrics from this rather than re-computing the
/// same values. [`Default`] — every field zero — is the report for a request
/// that was never measured at all, which is what a caller with no budget gets.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TruncationReport {
    /// Serialized payload size in bytes before truncation.
    pub payload_chars_before: usize,
    /// Serialized payload size in bytes after truncation. Equal to
    /// `payload_chars_before` when nothing was changed.
    pub payload_chars_after: usize,
    /// Number of messages whose `content` was replaced with
    /// [`TRUNCATION_PLACEHOLDER`].
    pub messages_truncated: usize,
}

impl TruncationReport {
    /// The report for a body that came through untouched.
    const fn unchanged(payload_chars: usize) -> Self {
        Self {
            payload_chars_before: payload_chars,
            payload_chars_after: payload_chars,
            messages_truncated: 0,
        }
    }
}

/// The request cannot be made to fit its context budget.
///
/// Surfaces map this to their own idiom — the proxy to HTTP 400
/// `context_length_exceeded`, the in-process agent path to an error on the
/// completion call.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TruncationError {
    /// Still over budget after every trimmable message was trimmed.
    #[error(
        "conversation is {payload_chars} characters after truncation, over the \
         {limit_chars}-character context budget"
    )]
    ExceedsBudgetAfterTruncation {
        /// Serialized payload size once trimming could do no more.
        payload_chars: usize,
        /// The budget it still exceeds.
        limit_chars: usize,
    },
}

// =============================================================================
// The stage
// =============================================================================

/// Trim stale history in place so the request fits within `limit_chars`,
/// aiming past the budget for the [`LOW_WATERMARK_PCT`] watermark once
/// triggered.
///
/// `limit_chars` is the total payload character budget for this request. See
/// the [module documentation](self) for the full algorithm.
///
/// # Errors
///
/// [`TruncationError::ExceedsBudgetAfterTruncation`] when the payload still
/// exceeds the budget after every eligible message has been trimmed. `body` is
/// left in its trimmed state; callers reject the request rather than forward it.
pub fn truncate_history(
    body: &mut Value,
    limit_chars: usize,
) -> Result<TruncationReport, TruncationError> {
    let payload_chars_before = serialized_len(body);

    // ── Budget gate ──────────────────────────────────────────────────────────
    // While the whole payload fits there is nothing to do: leave it alone and
    // keep the model's full history intact. This is the common case, and the
    // reason truncation does not mutilate history pre-emptively.
    if payload_chars_before <= limit_chars {
        return Ok(TruncationReport::unchanged(payload_chars_before));
    }

    // Zero blast radius: a body this stage does not understand passes through
    // rather than being rejected on a measurement it cannot act on.
    let Some(messages) = body.get_mut("messages").and_then(Value::as_array_mut) else {
        return Ok(TruncationReport::unchanged(payload_chars_before));
    };

    // ── Oldest-first trim ────────────────────────────────────────────────────
    // Walk from the oldest message toward the newest, eliding eligible
    // oversized content until the cumulative estimated savings reach the
    // quantized watermark target, so the freshest tool outputs are the last to
    // be sacrificed. The budget gate above guarantees the target is positive,
    // so the loop never stops before considering the first eligible message.
    // Exhausting the eligible messages short of the target is fine: the hard
    // abort below fires only when the payload still exceeds the budget itself,
    // not the watermark.
    let total = messages.len();
    let placeholder_len = TRUNCATION_PLACEHOLDER.len();
    let target_savings = target_savings_chars(payload_chars_before, limit_chars);
    let mut messages_truncated = 0usize;
    let mut saved = 0usize;

    for (i, msg) in messages.iter_mut().enumerate() {
        if saved >= target_savings {
            break;
        }
        // Tail-protected messages and non-candidate roles (system, user) are
        // skipped entirely.
        if is_tail_protected(i, total) || !is_truncation_candidate(msg) {
            continue;
        }

        // Only string-form content is replaced. Array-form content (multi-part
        // messages) is left untouched, as is `tool_calls` at any role.
        let Some(content_len) = msg
            .get("content")
            .and_then(Value::as_str)
            .map(str::len)
            .filter(|len| *len > TOOL_CONTENT_THRESHOLD_CHARS)
        else {
            continue;
        };

        msg["content"] = Value::String(TRUNCATION_PLACEHOLDER.to_owned());
        messages_truncated += 1;
        // Each replacement reclaims (content_len - placeholder_len) chars.
        saved = saved.saturating_add(content_len.saturating_sub(placeholder_len));
    }

    // ── Budget check ─────────────────────────────────────────────────────────
    // Re-measure only when something actually changed; `saved` is an estimate
    // and the hard abort deserves the real number.
    let payload_chars_after = if messages_truncated == 0 {
        payload_chars_before
    } else {
        serialized_len(body)
    };

    if payload_chars_after > limit_chars {
        return Err(TruncationError::ExceedsBudgetAfterTruncation {
            payload_chars: payload_chars_after,
            limit_chars,
        });
    }

    Ok(TruncationReport {
        payload_chars_before,
        payload_chars_after,
        messages_truncated,
    })
}

// =============================================================================
// Helpers
// =============================================================================

/// The number of estimated characters truncation must reclaim from a payload
/// of `payload_chars` under a budget of `limit_chars`.
///
/// The naive answer — `payload_chars - limit_chars`, just enough to fit —
/// re-elides one more message on almost every turn of a growing conversation,
/// because this stage is stateless and the client resends the full raw history
/// each time. The target is therefore anchored at the [`LOW_WATERMARK_PCT`]
/// watermark and rounded **up** to a whole multiple of the margin between
/// watermark and budget: the result depends on the payload size only through
/// that coarse bracket, so the elision set it induces stays identical across
/// every turn within one margin (~25% of budget) of payload growth. See the
/// module docs for the full KV-cache rationale.
///
/// Returns `0` when the payload is already at or below the watermark. The
/// margin is clamped to at least one character so a degenerate `limit_chars`
/// of `0` still yields a finite target (the whole payload).
const fn target_savings_chars(payload_chars: usize, limit_chars: usize) -> usize {
    let watermark = limit_chars.saturating_mul(LOW_WATERMARK_PCT) / 100;
    let margin = match limit_chars - watermark {
        0 => 1,
        margin => margin,
    };
    let needed = payload_chars.saturating_sub(watermark);
    needed.div_ceil(margin).saturating_mul(margin)
}

/// Byte length of `body` once serialized, without allocating a copy of it.
///
/// The budget is denominated in wire bytes, and a [`Value`] has none until it
/// is serialized — but a 200 KB conversation does not need to be materialized
/// twice just to be measured.
fn serialized_len(body: &Value) -> usize {
    let mut counter = CountingWriter::default();
    // Serializing a `Value` cannot fail: it holds no non-string map keys and no
    // non-finite numbers, and the sink never errors. Reporting zero on that
    // unreachable branch degrades to "under budget", i.e. passthrough.
    if serde_json::to_writer(&mut counter, body).is_err() {
        return 0;
    }
    counter.0
}

/// An [`std::io::Write`] sink that keeps the byte count and discards the bytes.
#[derive(Default)]
struct CountingWriter(usize);

impl std::io::Write for CountingWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0 += buf.len();
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Returns `true` if the message at `index` (in a list of `total` messages)
/// falls within the protected tail window and must not be truncated.
#[inline]
const fn is_tail_protected(index: usize, total: usize) -> bool {
    index >= total.saturating_sub(PROTECTED_TAIL_COUNT)
}

/// Returns `true` if this message's role is eligible for content truncation.
///
/// Only `role: "tool"` and `role: "assistant"` are candidates. `role: "system"`
/// and `role: "user"` are never truncated.
#[inline]
fn is_truncation_candidate(msg: &Value) -> bool {
    matches!(
        msg.get("role").and_then(Value::as_str).unwrap_or(""),
        "tool" | "assistant"
    )
}

#[cfg(test)]
#[path = "truncation_tests.rs"]
mod truncation_tests;
