//! The most tokens one agent-loop turn may take, when nothing named a number.
//!
//! llama-server's own default is no limit (`n_predict: -1`), so a turn the
//! model never ends runs until the context does: tens of thousands of tokens
//! of a call the loop cannot see, for most of an hour. A loop's request that
//! leaves the ladder with no `max_tokens` is given [`DEFAULT_TURN_TOKENS`],
//! or the `reasoning` recipe's 32768 for a `reasoning`-tagged model, or more
//! when its thinking budget needs the room. A request that names one keeps
//! it, whoever named it: the person, a stored setting, or the model's recipe
//! (the `reasoning` recipe's 32768 among them).
//!
//! A turn that reaches the limit while it writes a tool call, or before it
//! writes any text, ends the run (`gglib_agent`'s `cut_off`), so the limit
//! is where a runaway turn stops; one cut off in its text is taken as if it
//! had stopped. The words after a Draw's picture are the exception and are
//! always answered for.
//!
//! A send to another machine is not limited here: that machine's proxy
//! resolves its own settings for the model, and a `max_tokens` from this
//! side would outrank them. A send through this machine's endpoint is, once
//! the endpoint's ladder is known to name none ([`limit_proxied_turn`]).

use gglib_core::domain::{InferenceConfig, capability_tags};
use gglib_core::request_pipeline::{self, ModelContext, SamplingLayers};
use serde_json::Value;

/// A turn's limit when nothing names one and it has no thinking budget.
///
/// The thinking room of the profile ladder's `high` rung, which is for
/// "multi-step work where the thinking is the point" (`inference_profile`):
/// an unbudgeted turn gets that much for its thinking and its answer or
/// call together. An eighth of a 131072-token context.
pub(super) const DEFAULT_TURN_TOKENS: u32 = 16_384;

/// The room a turn with a thinking budget is given past it, for the answer
/// or the call that follows the thinking.
pub(super) const ANSWER_TOKENS: u32 = 8_192;

/// A `reasoning`-tagged model's limit when nothing names one: the
/// `reasoning` recipe's `max_tokens`, which such a model gets anyway unless
/// a recipe of its own (an author's published one, or a measured one)
/// stands in the recipe's place and names no `max_tokens`.
pub(super) fn reasoning_turn_tokens() -> u32 {
    InferenceConfig::reasoning_profile()
        .max_tokens
        .unwrap_or(DEFAULT_TURN_TOKENS)
}

/// Give `body` its limit when it names no `max_tokens`: the larger of the
/// base ([`DEFAULT_TURN_TOKENS`], or [`reasoning_turn_tokens`] when
/// `reasoning`) and its `reasoning_budget_tokens` plus [`ANSWER_TOKENS`].
/// A budget that is not above zero (`-1` defers to the launch default, `0`
/// is no thinking) adds nothing.
pub(super) fn limit_turn(body: &mut Value, reasoning: bool) {
    let Some(obj) = body.as_object_mut() else {
        return;
    };
    if obj.contains_key("max_tokens") {
        return;
    }
    let base = if reasoning {
        reasoning_turn_tokens()
    } else {
        DEFAULT_TURN_TOKENS
    };
    let thinking = obj
        .get("reasoning_budget_tokens")
        .and_then(Value::as_u64)
        .and_then(|budget| u32::try_from(budget).ok())
        .unwrap_or(0);
    let limit = base.max(thinking.saturating_add(ANSWER_TOKENS));
    obj.insert("max_tokens".to_owned(), Value::from(limit));
}

/// [`limit_turn`] for a body this machine's endpoint will fold.
///
/// The endpoint takes `max_tokens` as the client's own budget, which it
/// honours whether or not it trusts clients' sampling, so a limit written
/// here outranks a `max_tokens` its ladder would name (a stored setting, the
/// model's recipe). The ladder is therefore folded on a copy, with the model
/// and the stored `layers` the endpoint folds with, and the limit is written
/// only when that fold names none: a turn ends at the number the ladder
/// names, as it did when the fold was this side's.
pub(super) fn limit_proxied_turn(body: &mut Value, model: &ModelContext, layers: &SamplingLayers) {
    let mut folded = body.clone();
    let layers = SamplingLayers {
        trust_client_sampling: true,
        ..layers.clone()
    };
    let decision = request_pipeline::resolve_sampling(&mut folded, model, &layers);
    if decision.resolved.max_tokens.is_some() {
        return;
    }
    limit_turn(&mut folded, capability_tags::is_reasoning(&model.tags));
    if let Some(limit) = folded.get("max_tokens").cloned() {
        body["max_tokens"] = limit;
    }
}

#[cfg(test)]
#[path = "turn_limit_tests.rs"]
mod tests;
