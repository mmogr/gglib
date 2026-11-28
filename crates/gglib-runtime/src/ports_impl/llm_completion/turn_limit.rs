//! The most tokens one agent-loop turn may take, when nothing named a number,
//! and the thinking budget that leaves room in it for the answer.
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
//! So a turn whose limit is gglib's own, and whose ladder names no
//! `reasoning_budget_tokens`, is also given a thinking budget of that limit
//! less [`ANSWER_TOKENS`]: 8192 under [`DEFAULT_TURN_TOKENS`], 24576 under
//! the `reasoning` recipe's limit. It is the mirror of the room a named
//! budget is given past it. gglib's own is read from the fold's record of
//! where each value came from ([`SamplingDecision::sources`]): the limit
//! written here because no rung named one, or the built-in `reasoning`
//! recipe's, on the ladder's bottom rung. A limit any other rung named
//! derives nothing, so a small one never becomes a budget, and a budget any
//! rung named is never touched, `0` and `-1` among them. llama-server's
//! sampler enforces the budget; gglib counts nothing, logs nothing and sends
//! no `reasoning_budget_message` ([ADR 0007]).
//!
//! A send to another machine is not limited here: that machine's proxy
//! resolves its own settings for the model, and a `max_tokens` from this
//! side would outrank them. A send through this machine's endpoint is, once
//! the endpoint's ladder is known to name none ([`limit_proxied_turn`]), and
//! is given the budget once that ladder names none.
//!
//! [ADR 0007]: https://github.com/mmogr/gglib/blob/main/docs/adr/0007-ask-the-server-for-template-capabilities.md

use gglib_core::domain::{InferenceConfig, ModelSamplingContext, ParamSource, capability_tags};
use gglib_core::request_pipeline::{
    self, LADDER_RUNGS, ModelContext, SamplingDecision, SamplingLayers,
};
use serde_json::Value;

/// A turn's limit when nothing names one and it has no thinking budget.
///
/// The thinking room of the profile ladder's `high` rung, which is for
/// "multi-step work where the thinking is the point" (`inference_profile`):
/// an unbudgeted turn gets that much for its thinking and its answer or
/// call together. An eighth of a 131072-token context.
pub(super) const DEFAULT_TURN_TOKENS: u32 = 16_384;

/// The room a turn's limit keeps past its thinking budget, for the answer
/// or the call that follows the thinking: added to a named budget, and taken
/// from gglib's own limit to give the budget it derives.
pub(super) const ANSWER_TOKENS: u32 = 8_192;

/// The wire key of the thinking budget.
const BUDGET_KEY: &str = "reasoning_budget_tokens";

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
/// is no thinking) adds nothing. Returns the limit it wrote, if it wrote one.
pub(super) fn limit_turn(body: &mut Value, reasoning: bool) -> Option<u32> {
    let obj = body.as_object_mut()?;
    if obj.contains_key("max_tokens") {
        return None;
    }
    let base = if reasoning {
        reasoning_turn_tokens()
    } else {
        DEFAULT_TURN_TOKENS
    };
    let thinking = obj
        .get(BUDGET_KEY)
        .and_then(Value::as_u64)
        .and_then(|budget| u32::try_from(budget).ok())
        .unwrap_or(0);
    let limit = base.max(thinking.saturating_add(ANSWER_TOKENS));
    obj.insert("max_tokens".to_owned(), Value::from(limit));
    Some(limit)
}

/// The limit and the thinking budget of a loop turn sent straight to
/// llama-server, whose ladder `decision` folded into `body`.
pub(super) fn limit_loop_turn(body: &mut Value, model: &ModelContext, decision: &SamplingDecision) {
    let written = limit_turn(body, capability_tags::is_reasoning(&model.tags));
    if let Some(limit) = gglibs_own_limit(written, model, decision) {
        bound_thinking(body, limit, decision);
    }
}

/// [`limit_loop_turn`] for a body this machine's endpoint will fold.
///
/// The endpoint takes `max_tokens` and `reasoning_budget_tokens` as the
/// client's own budgets, which it honours whether or not it trusts clients'
/// sampling, so a number written here outranks one its ladder would name (a
/// profile, a stored setting, the model's recipe). The ladder is therefore
/// folded on a copy, with the model and the stored `layers` the endpoint
/// folds with, and each is written only when that fold names none: a turn
/// ends at the numbers the ladder names, as it did when the fold was this
/// side's. The built-in recipe's limit is the endpoint's to send, and the
/// budget it derives is written here.
pub(super) fn limit_proxied_turn(body: &mut Value, model: &ModelContext, layers: &SamplingLayers) {
    let mut folded = body.clone();
    let layers = SamplingLayers {
        trust_client_sampling: true,
        ..layers.clone()
    };
    let decision = request_pipeline::resolve_sampling(&mut folded, model, &layers);
    let written = limit_turn(&mut folded, capability_tags::is_reasoning(&model.tags));
    if let Some(limit) = written {
        body["max_tokens"] = Value::from(limit);
    }
    if let Some(limit) = gglibs_own_limit(written, model, &decision) {
        bound_thinking(body, limit, &decision);
    }
}

/// The turn's limit when it is gglib's own: the one [`limit_turn`] wrote
/// because no rung named `max_tokens`, or the built-in `reasoning` recipe's
/// from the ladder's bottom rung. `None` for a limit any other rung named.
fn gglibs_own_limit(
    written: Option<u32>,
    model: &ModelContext,
    decision: &SamplingDecision,
) -> Option<u32> {
    match decision.sources.max_tokens {
        ParamSource::Unset => written,
        ParamSource::Layer(rung) if rung == LADDER_RUNGS - 1 && on_the_built_in_recipe(model) => {
            decision.resolved.max_tokens
        }
        ParamSource::Layer(_) | ParamSource::Coupled => None,
    }
}

/// Whether the ladder's bottom rung holds the built-in `reasoning` recipe
/// rather than a recipe stored on the model's row (an author's published
/// one, or a measured one): read from the rungs the fold placed, which put
/// either the stored config or the built-in recipe there.
fn on_the_built_in_recipe(model: &ModelContext) -> bool {
    let stored = model.inference_defaults.as_ref();
    let rungs = ModelSamplingContext {
        is_reasoning: capability_tags::is_reasoning(&model.tags),
        defaults_origin: model.defaults_origin,
    }
    .model_rungs(stored);
    rungs
        .below_global
        .is_some_and(|bottom| !stored.is_some_and(|stored| std::ptr::eq(bottom, stored)))
}

/// Give `body` a thinking budget of `limit` less [`ANSWER_TOKENS`], unless
/// the ladder or the body already names one.
fn bound_thinking(body: &mut Value, limit: u32, decision: &SamplingDecision) {
    if decision.sources.reasoning_budget_tokens != ParamSource::Unset {
        return;
    }
    let Some(obj) = body.as_object_mut() else {
        return;
    };
    if obj.contains_key(BUDGET_KEY) {
        return;
    }
    if let Some(budget) = limit
        .checked_sub(ANSWER_TOKENS)
        .filter(|budget| *budget > 0)
    {
        obj.insert(BUDGET_KEY.to_owned(), Value::from(budget));
    }
}

#[cfg(test)]
#[path = "turn_limit_tests.rs"]
mod tests;
