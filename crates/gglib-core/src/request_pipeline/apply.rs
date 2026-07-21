//! The ordered request-shaping pipeline, and the one statement of its order.
//!
//! # The stages
//!
//! | # | Stage | Lives in | Reads |
//! |---|---|---|---|
//! | 1 | Strip prior reasoning | [`super::messages`] | `messages` |
//! | 2 | Coalesce for capabilities | [`super::messages`] | `messages` |
//! | 3 | *(history truncation — still proxy-local, see below)* | | |
//! | 4 | Resolve the sampling hierarchy | [`super::sampling`] | top-level keys |
//! | 5 | Pin `cache_prompt` | [`super::sampling`] | top-level keys |
//!
//! # The order is load-bearing
//!
//! **1 before 2.** Coalescing merges message *content*. Stripping afterwards
//! would have to find and excise `<think>` blocks inside text that has already
//! been concatenated with `"\n\n"` separators from other turns.
//!
//! **2 before 4.** There is no data dependency — stage 4 never looks at
//! `messages` and stages 1–2 never look at top-level keys — but this is the
//! order the proxy has always applied, and keeping it removes the question of
//! whether any future stage introduced one.
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
//! # Why stage 3 is not here yet
//!
//! History truncation gates on the payload's size **in wire bytes** and can
//! reject the request outright with an HTTP response. Neither fits a
//! `&mut Value` pipeline in `gglib-core`: a `Value` has no byte length until it
//! is serialized, and `gglib-core` cannot name an `axum` type. It therefore
//! still runs in `gglib-proxy`, between stages 2 and 4, which is why the proxy
//! calls [`super::shape_messages`] and [`super::resolve_sampling`] separately
//! rather than calling [`apply`]. Splicing it in the middle is not cosmetic:
//! inserting the stage-4 sampling keys first would change the very number
//! truncation measures.

use serde_json::Value;

use super::{ModelContext, SamplingLayers, messages, sampling};

/// Apply every request-shaping transform, in order, in place.
///
/// This is the whole pipeline as one call — the entry point for any caller that
/// holds a request body and nothing else to interleave. See the [module
/// docs](self) for the stage order and why it is fixed.
///
/// Unknown fields, top-level and per-message alike, are preserved.
pub fn apply(body: &mut Value, ctx: &ModelContext, layers: &SamplingLayers) {
    messages::shape_messages(body, ctx);
    sampling::resolve_sampling(body, ctx, layers);
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

    /// The contract the proxy depends on: it runs the two stages by hand, with
    /// truncation between them, and must land exactly where `apply` would.
    /// If a stage is ever added to `apply` without the proxy learning about it,
    /// this fails.
    #[test]
    fn apply_equals_the_stages_run_in_sequence() {
        let ctx = strict_turn_ctx();
        let layers = SamplingLayers {
            profile: Some(InferenceConfig {
                top_p: Some(0.5),
                ..Default::default()
            }),
            global: None,
        };

        let mut via_apply = kitchen_sink();
        apply(&mut via_apply, &ctx, &layers);

        let mut via_stages = kitchen_sink();
        messages::shape_messages(&mut via_stages, &ctx);
        sampling::resolve_sampling(&mut via_stages, &ctx, &layers);

        assert_eq!(via_apply, via_stages);
    }

    /// Stage 1 must have already run when stage 2 merges: the merged text
    /// contains no `<think>` remnant, which it would if the order were flipped.
    #[test]
    fn reasoning_is_stripped_before_messages_are_merged() {
        let mut body = kitchen_sink();
        apply(&mut body, &strict_turn_ctx(), &SamplingLayers::default());

        let merged = body["messages"][0]["content"].as_str().unwrap();
        assert_eq!(merged, "a\n\nb");
        assert!(body["messages"][0].get("reasoning_content").is_none());
    }

    #[test]
    fn every_stage_runs_in_one_call() {
        let mut body = kitchen_sink();
        apply(&mut body, &strict_turn_ctx(), &SamplingLayers::default());

        // 1 + 2: reasoning gone, assistant turns merged, tool turn intact.
        assert_eq!(body["messages"].as_array().unwrap().len(), 2);
        assert_eq!(body["messages"][1]["tool_call_id"], "call_1");
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
        );

        assert_eq!(
            body["messages"].as_array().unwrap().len(),
            2,
            "unknown capabilities must not merge anything"
        );
        assert_eq!(body["cache_prompt"], true);
        assert!(body["temperature"].as_f64().is_some());
    }
}
