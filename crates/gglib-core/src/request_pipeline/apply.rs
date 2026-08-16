//! The ordered request-shaping pipeline, and the one statement of its order.
//!
//! # The stages
//!
//! | # | Stage | Lives in | Reads |
//! |---|---|---|---|
//! | 1 | Strip prior reasoning | [`super::messages`] | `messages` |
//! | 2 | Coalesce for capabilities | [`super::messages`] | `messages` |
//! | 2b | Strip unsupported tools | [`super::tools`] | `tools`, capabilities |
//! | 3 | Truncate stale history | [`super::truncation`] | `messages`, payload size |
//! | 4 | Resolve the sampling hierarchy | [`super::sampling`] | top-level keys |
//! | 5 | Pin `cache_prompt` | [`super::sampling`] | top-level keys |
//! | 6 | Constrain dialect tool calls | [`super::constrain`] | `tools`, `tool_choice`, tags |
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
//! The same goes for stage 2b: a stripped tools array must not count against
//! the truncation budget.
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
//! **6 after 3.** The grammar stage 6 *adds* a top-level key, so it runs
//! after the measurement for the same reason sampling does: the truncation
//! budget measures the client's conversation, not our own additions to it.
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

use super::sampling::SamplingDecision;
use super::truncation::{TruncationError, TruncationReport};
use super::{ModelContext, SamplingLayers, constrain, messages, sampling, tools, truncation};

/// What the pipeline did, for the caller that has to report or verify it.
///
/// Both halves were previously unavailable in different ways: truncation was
/// returned bare, and sampling was not returned at all — it went into a
/// `debug!` and nowhere else. Bundling them keeps one return value as stages
/// gain things worth saying.
#[derive(Debug, Clone, PartialEq)]
pub struct PipelineReport {
    /// Stage 3. Zeroed when `budget_chars` was `None` — the request was
    /// shaped but never measured.
    pub truncation: TruncationReport,
    /// Stages 4–5. See [`SamplingDecision`].
    pub sampling: SamplingDecision,
}

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
) -> Result<PipelineReport, TruncationError> {
    messages::shape_messages(body, ctx);
    tools::strip_unsupported_tools(body, ctx);

    let truncation = match budget_chars {
        Some(limit) => truncation::truncate_history(body, limit)?,
        None => TruncationReport::default(),
    };

    let sampling = sampling::resolve_sampling(body, ctx, layers);

    // Stage 6 runs unconditionally, because there is only one kind of trip
    // through this pipeline.
    //
    // A `PipelinePass` parameter used to exist so this stage could stand down
    // on a tool-call repair: `constrain` fires on `tool_choice: "required"`
    // for dialect models, installs gglib's own grammar and rewrites
    // `tool_choice` to `"none"` (llama-server rejects a custom grammar
    // alongside `tools`), which on a repair would silently convert the
    // re-issue into a request for no tool call at all.
    //
    // That guard was never reachable. The repair path does not call `apply`
    // at all — it mutates the already-resolved body and sends it — so every
    // caller here passed `Initial` and the alternative branch was dead. The
    // reasoning still matters, but it belongs where the risk actually lives:
    // see `gglib_proxy::repair::repair_body`, which must keep bypassing this
    // pipeline for exactly the reason above.
    constrain::constrain_tool_calls(body, ctx);
    Ok(PipelineReport {
        truncation,
        sampling,
    })
}

#[cfg(test)]
mod live_shape_probe {
    use super::*;
    use crate::domain::{DefaultsOrigin, InferenceConfig};

    /// The exact body and context shape of the live hardware check that
    /// found the gated-key passthrough, end to end through `apply` rather
    /// than through `resolve_sampling` alone.
    #[test]
    fn the_live_bodys_frequency_penalty_is_stripped() {
        let mut body = serde_json::json!({
            "model": "Qwen3.5-4B",
            "messages": [{"role": "user", "content": "Say hello briefly."}],
            "max_tokens": 20,
            "frequency_penalty": 0.9,
            "top_p": 0.3
        });
        let ctx = ModelContext {
            tags: vec!["agent".into(), "reasoning".into(), "mtp".into()],
            inference_defaults: Some(InferenceConfig::reasoning_profile()),
            defaults_origin: Some(DefaultsOrigin::AutoDetected),
            ..ModelContext::passthrough()
        };
        let layers = SamplingLayers::default();
        let report = apply(&mut body, &ctx, &layers, None).expect("pipeline applies");
        assert!(report.sampling.applied);

        let obj = body.as_object().unwrap();
        assert!(
            !obj.contains_key("frequency_penalty"),
            "frequency_penalty survived: {body}"
        );
        let top_p = obj["top_p"].as_f64().expect("recipe top_p present");
        assert!((top_p - 0.95).abs() < 1e-6, "client top_p survived: {body}");
    }
}

#[cfg(test)]
#[path = "apply_tests.rs"]
mod apply_tests;
