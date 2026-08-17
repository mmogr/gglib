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
//! | 5b | Suppress an unreadable `reasoning_effort` | [`super::effort_gate`] | resolved effort, template caps |
//! | — | Log the sampling decision | [`super::sampling_log`] | the decision |
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
//! **5b after 4.** Stage 5b deletes a `reasoning_effort` the model's observed
//! template never reads, and the value it has to catch is usually not the
//! client's. It arrives from the **ladder** — a `:high` profile, a per-model
//! default, a global setting — which does not exist until stage 4 has folded
//! it. Placed at 2b beside the tool strip, where the capability shape is
//! otherwise identical, the gate would delete the client's key and stage 4
//! would then force-insert gglib's own resolved level straight past it (the
//! patch is *inserted*, not merged), so the case that matters most would
//! sail through untouched while the tests still passed on a client-sent
//! level. The stage runs after 4 for the same reason it takes
//! `&mut SamplingDecision`: it can only suppress a value once something has
//! resolved one, and it must correct that decision's own record when it does.
//! `a_ladder_supplied_effort_is_suppressed_not_just_a_client_one` fails if
//! this ever moves.
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

use super::effort_gate::SuppressedEffort;
use super::sampling::SamplingDecision;
use super::truncation::{TruncationError, TruncationReport};
use super::{
    ModelContext, SamplingLayers, constrain, effort_gate, messages, sampling, sampling_log, tools,
    truncation,
};

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
    /// Stage 5b. `Some` when a resolved `reasoning_effort` was thrown away
    /// because the model's observed template does not read it.
    ///
    /// It lives here rather than on [`SamplingDecision`] because that type is
    /// *what `resolve_sampling` decided*, and this is what a later stage did
    /// to it — the same relationship [`truncation`](Self::truncation) has to
    /// stage 3. The decision is not left lying, though: stage 5b rewrites its
    /// `resolved` and `sources` in place, so a consumer holding only the
    /// `SamplingDecision` (the dashboard, the audit) still sees the value gone
    /// and its provenance reading
    /// [`SuppressedByTemplate`](crate::domain::ParamSource::SuppressedByTemplate).
    /// What only this field adds is the level that was dropped and the rung
    /// that asked for it.
    pub effort_suppressed: Option<SuppressedEffort>,
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

    let mut sampling = sampling::resolve_sampling(body, ctx, layers);

    // Stage 5b. After the fold, never before it: the level worth catching is
    // the one gglib itself resolved, and until stage 4 has run there is no
    // such value to catch. See the ordering rationale in the module docs.
    let effort_suppressed = effort_gate::suppress_unsupported_effort(body, ctx, &mut sampling);

    // Rendered here, not inside stage 4, so the one line that describes a
    // request's sampling describes what was *sent*. See `sampling_log`.
    sampling_log::log_resolution(&sampling);

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
        effort_suppressed,
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
