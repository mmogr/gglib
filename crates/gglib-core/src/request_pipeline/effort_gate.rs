//! Stage 5b: suppress a resolved `reasoning_effort` when the model's observed
//! template never reads it — and say so.
//!
//! [ADR 0007] decision 3: *the resolved `reasoning_effort` is suppressed when —
//! and only when — the observed caps positively say the template does not read
//! the variable*. This module is that sentence.
//!
//! # Why it must be reported rather than performed quietly
//!
//! A template that does not read the variable ignores it in perfect silence:
//! HTTP 200, prompt byte-identical, no warning, no status change (ADR 0007
//! finding 7c, confirmed live). Neither reasoning control is echoed anywhere —
//! not in `/slots.params`, not in `/props` (finding 7a) — so no readback will
//! ever notice that a level went nowhere. If gglib deletes the key without a
//! record, the fact is unrecoverable from every surface at once.
//!
//! So the suppression writes itself down twice.
//! [`ParamSource::SuppressedByTemplate`] replaces the rung in the decision's
//! provenance — no surface can print `reasoning_effort=profile` for a value
//! that was never sent — and [`SuppressedEffort`] carries the level and the
//! rung that supplied it, so a surface can say *which* value was dropped and
//! *who* asked for it.
//!
//! Both records are read downstream: `gglib model explain` renders the
//! provenance, and the proxy hands [`SuppressedEffort`] to its sampling audit so
//! the dashboard can name the level and the rung.
//!
//! This module's own `debug!` stays, and is not redundant with the pipeline's
//! `"sampling resolved"` line. That line is rendered by
//! [`sampling_log`](super::sampling_log) *after* this stage precisely so it
//! describes what was sent — which means that on a suppression it reads
//! `reasoning_effort=None … reasoning_effort=suppressed-by-template`, and the
//! level and rung this stage threw away appear nowhere in it.
//!
//! # Unknown never gates
//!
//! The predicate is deliberately conservative in the same shape
//! [`strip_unsupported_tools`](super::tools::strip_unsupported_tools) uses
//! (`tools.rs:29-40`): it acts only on a
//! [`catalog_resolved`](super::ModelContext::catalog_resolved) context, and
//! only on a positive [`Support::No`]. A passthrough model, a model nobody has
//! launched yet, a `/props` read that failed, and a caps object that did not
//! carry the field all mean *nobody knows*, and all keep their effort.
//!
//! The precedent is a shape, not an equivalence, and the difference is worth
//! stating: tool stripping reads **gglib's own catalog row** — a fact this
//! system recorded about the model — while this reads **llama-server's
//! self-report**, a fact another process stated about itself. ADR 0007 names
//! that posture (*a runtime self-report used as a policy input*) precisely
//! because it is not the same thing as a stored capability, and it carries the
//! extra rule that a report which failed to arrive must never be read as one
//! that arrived negative.
//!
//! # Scope: the top-level key, and only the effort
//!
//! This gate governs the top-level `reasoning_effort` key alone. A client's
//! `chat_template_kwargs` remains a **verbatim passthrough** — gglib neither
//! reads nor edits it, and a caller who puts an effort level in there is
//! addressing the template directly, over gglib's head, which is a different
//! (and unmodelled) act from setting the field the ladder resolves.
//!
//! [`reasoning_budget_tokens`](crate::domain::InferenceConfig::reasoning_budget_tokens)
//! is **never** suppressed here. It is not a template variable at all: it is
//! enforced by llama.cpp's own sampler-side budget
//! (`common/reasoning-budget.{h,cpp}`) and range-validated upstream, so a
//! template that ignores `reasoning_effort` still honours the budget. Gating it
//! on a caps bit that describes a *template* would be a category error, and
//! `the_budget_survives_a_model_that_cannot_honour_effort` pins it.
//!
//! [ADR 0007]: https://github.com/mmogr/gglib/blob/main/docs/adr/0007-ask-the-server-for-template-capabilities.md

use serde::Serialize;
use serde_json::Value;
use tracing::debug;

use super::ModelContext;
use super::sampling::SamplingDecision;
use crate::domain::inference::REASONING_EFFORT_KEY;
use crate::domain::{
    FieldSources, InferenceConfig, ParamSource, ReasoningEffort, Support, TemplateCaps,
    reasoning_effort_support,
};

/// A resolved effort level this stage threw away, and where it came from.
///
/// The provenance entry alone says *that* a value was suppressed; this says
/// **which** and **whose**. Both halves are needed for the sentence an operator
/// has to be able to read — "the `:high` profile asked for `high`; this model's
/// template does not read `reasoning_effort`, so nothing was sent" — and the
/// rung is the half that would otherwise be destroyed, because
/// [`ParamSource::SuppressedByTemplate`] overwrites it in
/// [`FieldSources`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SuppressedEffort {
    /// The level the ladder resolved, which llama-server never saw.
    pub level: ReasoningEffort,
    /// The rung that supplied it — a [`ParamSource::Layer`] index into
    /// [`SamplingDecision::layer_names`], since no floor ever names an effort
    /// (`no_floor_names_a_reasoning_control`).
    pub source: ParamSource,
}

/// Remove a resolved `reasoning_effort` the observed template cannot read, and
/// return what was removed.
///
/// Returns `None` — leaving `body` and `decision` untouched — for every model
/// that is not a positive "this template does not read it": unresolved
/// contexts, never-observed models, unreadable caps, caps that omit the field,
/// and templates that do read it. Also `None` when the ladder resolved no
/// effort at all, which is the ordinary case: nothing was going to be sent, so
/// nothing is suppressed and there is nothing to report.
///
/// On a suppression it does three things, and the second and third are the
/// point:
///
/// 1. deletes the top-level key from `body`;
/// 2. clears `decision.resolved.reasoning_effort`, because that field is
///    documented as *the values written into the body* and this one no longer
///    is;
/// 3. rewrites `decision.sources.reasoning_effort` to
///    [`ParamSource::SuppressedByTemplate`], so every provenance surface
///    reports the suppression instead of naming the rung whose value did not
///    survive.
pub fn suppress_unsupported_effort(
    body: &mut Value,
    ctx: &ModelContext,
    decision: &mut SamplingDecision,
) -> Option<SuppressedEffort> {
    // `catalog_resolved` is stated here rather than folded into the shared
    // predicate below because it is this caller's question, not the rule's: a
    // passthrough request is one gglib knows nothing about, and the caps field
    // on such a context is `None` for want of a lookup rather than for want of
    // an observation. The explain surfaces have no equivalent doubt — they are
    // holding the catalog row.
    if !ctx.catalog_resolved {
        return None;
    }
    // Checked before anything is mutated: a body that is not a JSON object is
    // left alone everywhere in this pipeline, and a decision recording a
    // suppression that did not happen to a body is worse than no record.
    if !body.is_object() {
        return None;
    }

    let suppressed = suppress_stored_effort(
        &mut decision.resolved,
        &mut decision.sources,
        &ctx.template_caps,
    )?;

    if let Some(obj) = body.as_object_mut() {
        obj.remove(REASONING_EFFORT_KEY);
    }

    // `debug!`, not `warn!`: on a model whose template ignores the variable
    // this fires on every request that resolves a level, and the condition is
    // a property of the model, not a fault. It is logged at all because the
    // wire will never show it — see the module docs.
    debug!(
        level = %suppressed.level,
        from = %describe_rung(suppressed.source, &decision.layer_names),
        "reasoning_effort suppressed: this model's template does not read it"
    );
    Some(suppressed)
}

/// Stage 5b's rule, applied to a resolution with no request in hand.
///
/// The predicate and both record-keeping edits, minus everything that needs a
/// body: [`suppress_unsupported_effort`] is this plus the key deletion and the
/// log line. Split out because `gglib model explain` and
/// `GET /api/models/:id/explain` have to answer the same question about the
/// same model and must not answer it differently. An explain surface that
/// re-implemented the condition could only ever *disagree* with the gate it is
/// describing — the same argument ADR 0007 makes for reading llama-server's
/// self-report instead of building a detector, one level in.
///
/// Note what an explain surface is doing when it calls this: it is reporting a
/// **conditional** fact. The stored configuration resolves a level, and on any
/// real request against this model that level would be deleted before sending.
/// Nothing has been sent, and nothing here pretends otherwise — which is why
/// the surfaces render it as *would not be sent*, not as *was not sent*.
///
/// `catalog_resolved` has no analogue here and is not needed: a caller holding
/// a model row has, by construction, resolved the catalog. `caps` being `None`
/// still means "never observed" and still answers [`Support::Unknown`], so an
/// unlaunched model keeps its level exactly as an unresolved request does.
///
/// Returns `None` — leaving both arguments untouched — unless the caps
/// positively say the template does not read the variable *and* something
/// resolved a level to suppress.
#[must_use]
pub fn suppress_stored_effort(
    resolved: &mut InferenceConfig,
    sources: &mut FieldSources,
    caps: &Option<TemplateCaps>,
) -> Option<SuppressedEffort> {
    let level = resolved.reasoning_effort?;

    // `is_some` is stated even though `reasoning_effort_support` already
    // answers `Unknown` for absent caps: this is the one predicate in the arc
    // allowed to delete a value, and it should read as the conjunction ADR 0007
    // decision 3 writes rather than lean on a helper's behaviour at a distance.
    if caps.is_none() || reasoning_effort_support(caps) != Support::No {
        return None;
    }

    let suppressed = SuppressedEffort {
        level,
        source: sources.reasoning_effort,
    };
    resolved.reasoning_effort = None;
    sources.reasoning_effort = ParamSource::SuppressedByTemplate;
    Some(suppressed)
}

/// Name the rung a suppressed level came from, for the debug line.
///
/// A floor label is unreachable — no floor names an effort — but the arm is
/// spelled out rather than guessed at, and a value already suppressed cannot
/// be suppressed twice.
fn describe_rung(source: ParamSource, names: &[&'static str]) -> &'static str {
    match source {
        ParamSource::Layer(i) => names.get(i).copied().unwrap_or("?"),
        ParamSource::Floor | ParamSource::FloorCoupled => "floor",
        ParamSource::Unset => "unset",
        ParamSource::SuppressedByTemplate => "suppressed-by-template",
    }
}

#[cfg(test)]
#[path = "effort_gate_tests.rs"]
mod effort_gate_tests;

#[cfg(test)]
#[path = "effort_gate_stored_tests.rs"]
mod effort_gate_stored_tests;
