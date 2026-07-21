//! The ordered request-shaping pipeline, and the one statement of its order.
//!
//! # The stages
//!
//! | # | Stage | Lives in | Reads |
//! |---|---|---|---|
//! | 1 | Strip prior reasoning | [`super::messages`] | `messages` |
//! | 2 | Coalesce for capabilities | [`super::messages`] | `messages` |
//! | 3 | Truncate stale history | [`super::truncation`] | `messages`, payload size |
//! | 4 | Resolve the sampling hierarchy | [`super::sampling`] | top-level keys |
//! | 5 | Pin `cache_prompt` | [`super::sampling`] | top-level keys |
//!
//! # The order is load-bearing
//!
//! **1 before 2.** Coalescing merges message *content*. Stripping afterwards
//! would have to find and excise `<think>` blocks inside text that has already
//! been concatenated with `"\n\n"` separators from other turns.
//!
//! **2 before 3.** Both stages 1 and 2 only ever shrink the body, and stage 3
//! measures it. Truncating first would size its budget against bytes that were
//! about to be discarded anyway, and trim history that did not need trimming.
//!
//! **3 before 4.** Stage 3 measures the payload; stage 4 inserts up to seven
//! sampling keys. Resolving sampling first would have truncation size its
//! budget against keys the client never sent. The margin is small, but it is
//! the difference between measuring the conversation and measuring our own
//! additions to it.
//!
//! **4 before 5.** `cache_prompt` is not an [`InferenceConfig`] field, so
//! pinning it last means the resolved sampling patch can never overwrite it.
//!
//! [`InferenceConfig`]: crate::domain::InferenceConfig
//!
//! # Why the seam is `&mut Value` and not a typed request struct
//!
//! The proxy forwards requests from arbitrary external clients — IDE
//! extensions, gateways — which send `OpenAI` parameters this workspace has
//! never heard of. Round-tripping through a typed `ChatRequest` would silently
//! drop every field the struct does not model: a passthrough regression that
//! is invisible in tests and painful in the field. Mutating a `Value` in place
//! preserves them by construction. The adapter builds its body with `json!` and
//! already holds a `Value`, so this is also the cheaper side for it.
//!
//! # One pipeline, two callers, no second route
//!
//! Every request path calls [`apply`]. The proxy used to run the stages by hand
//! with its own truncation pass spliced between them, because truncation gated
//! on the payload's size in **wire bytes** and could reject the request with an
//! `axum` response — neither of which fits here. Measuring the serialized
//! `Value` and returning a domain error removed both obstacles, so there is now
//! exactly one implementation of the order above and nothing to keep in sync.

use serde_json::Value;

use super::truncation::{TruncationError, TruncationReport};
use super::{ModelContext, SamplingLayers, messages, sampling, truncation};

/// Apply every request-shaping transform, in order, in place.
///
/// This is the whole pipeline as one call. See the [module docs](self) for the
/// stage order and why it is fixed.
///
/// `budget_chars` is the history-truncation budget in characters.
/// [`ModelContext::context_budget_chars`] is the answer for callers with no
/// live serving context to measure; the proxy passes its own, computed from the
/// running server's context size and a learned chars-per-token ratio. `None`
/// skips stage 3 entirely and reports zeroes — the request is shaped but never
/// measured, which is what an unresolvable model gets.
///
/// Unknown fields, top-level and per-message alike, are preserved.
///
/// # Errors
///
/// [`TruncationError`] when the conversation cannot be made to fit
/// `budget_chars`. `body` is left shaped and trimmed; callers reject the
/// request rather than forward it.
pub fn apply(
    body: &mut Value,
    ctx: &ModelContext,
    layers: &SamplingLayers,
    budget_chars: Option<usize>,
) -> Result<TruncationReport, TruncationError> {
    messages::shape_messages(body, ctx);

    let report = match budget_chars {
        Some(limit) => truncation::truncate_history(body, limit)?,
        None => TruncationReport::default(),
    };

    sampling::resolve_sampling(body, ctx, layers);
    Ok(report)
}

#[cfg(test)]
mod tests {
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
        assert_eq!(report.messages_truncated, 0);
        assert!(report.payload_chars_before > 0);
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

        assert_eq!(report.messages_truncated, 1);
        assert!(report.payload_chars_after <= 20_000);
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

        assert_eq!(report, TruncationReport::default());
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
}
