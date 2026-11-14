//! The ordered request-shaping pipeline, and the one statement of its order.
//!
//! # The stages
//!
//! | # | Stage | Lives in | Reads |
//! |---|---|---|---|
//! | 1 | Strip prior reasoning, unless the template keeps it | [`super::messages`] | `messages`, template caps |
//! | 1b | Note a drawn picture | [`super::drawn_note`] | `messages`, `tools` |
//! | 2 | Coalesce for capabilities | [`super::messages`] | `messages` |
//! | 2b | Strip unsupported tools | [`super::tools`] | `tools`, capabilities |
//! | 3 | Truncate stale history | [`super::truncation`] | `messages`, payload size |
//! | 4 | Resolve the sampling hierarchy | [`super::sampling`] | top-level keys |
//! | — | Log the sampling decision | [`super::sampling_log`] | the decision |
//!
//! Stages 5 and 6 have gone (ADR 0022); their numbers are not reused.
//!
//! # The order is load-bearing
//!
//! **1 before 2.** Coalescing merges message *content*. Stripping afterwards
//! would have to find and excise `<think>` blocks inside text that has already
//! been concatenated with `"\n\n"` separators from other turns.
//!
//! **1b before 2.** The note may be a `system` message, and stage 2 is what
//! turns one into a user message for a model with no system role and merges
//! same-role neighbours for a strict-turns model. Added afterwards, the note
//! would reach those models in a shape their templates refuse.
//!
//! **1b before 3.** The note is text gglib adds to the conversation the model
//! reads, so stage 3 measures it with the rest.
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
//! # One pipeline, three callers
//!
//! Outside tests, three paths call [`apply`]: the proxy's forwarding path
//! (`gglib-proxy`'s `forward.rs`), the runtime's completion adapter
//! (`gglib-runtime`'s `llm_completion`) and the daemon's request for a chat's
//! title (`gglib-axum`'s `handlers/chat_title.rs`). None runs these stages in
//! an order of its own. Truncation measures the serialized `Value` and
//! returns a domain error rather than an `axum` response, so the proxy needs
//! no truncation pass of its own.

use serde_json::Value;

use super::measure::ContextBudget;
use super::sampling::SamplingDecision;
use super::truncation::{TruncationError, TruncationReport};
use super::{ModelContext, SamplingLayers, messages, sampling, sampling_log, tools, truncation};

/// What the pipeline did, for the caller that has to report or verify it.
///
/// Bundled so there is one return value as stages gain things worth saying.
#[derive(Debug, Clone, PartialEq)]
pub struct PipelineReport {
    /// Stage 3. Zeroed when `budget` was `None` — the request was
    /// shaped but never measured.
    pub truncation: TruncationReport,
    /// Stage 4. See [`SamplingDecision`].
    pub sampling: SamplingDecision,
}

/// Apply every request-shaping transform, in order, in place.
///
/// This is the whole pipeline as one call. See the [module docs](self) for the
/// stage order and why it is fixed.
///
/// `budget` is the history-truncation budget.
/// [`ModelContext::context_budget`] is the answer for callers with no
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
/// `budget`. `body` is left shaped and trimmed; callers reject the
/// request rather than forward it.
pub fn apply(
    body: &mut Value,
    ctx: &ModelContext,
    layers: &SamplingLayers,
    budget: Option<ContextBudget>,
) -> Result<PipelineReport, TruncationError> {
    messages::shape_messages(body, ctx);
    tools::strip_unsupported_tools(body, ctx);

    let truncation = match budget {
        Some(budget) => truncation::truncate_history(body, budget)?,
        None => TruncationReport::default(),
    };

    let sampling = sampling::resolve_sampling(body, ctx, layers);
    sampling_log::log_resolution(&sampling);
    Ok(PipelineReport {
        truncation,
        sampling,
    })
}

#[cfg(test)]
mod live_shape_probe {
    use super::*;

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
