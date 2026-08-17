//! Tests for [`super::suppress_stored_effort`] — stage 5b's rule applied to a
//! resolution with no request in hand.
//!
//! A second sibling to [`super`] rather than more cases in
//! `effort_gate_tests`: that module's own header explains that it runs through
//! [`apply`] on purpose, because the live gate's characteristic defect is a
//! *placement* defect. These do the opposite — they call the predicate directly
//! — and the last test here is the one that joins the two.
//!
//! Split for the file budget as much as for the framing; `effort_gate_tests`
//! was already within thirty lines of it.

use super::*;
use crate::domain::{FieldSources, InferenceConfig, ReasoningEffort, TemplateCaps};
use crate::request_pipeline::{SamplingLayers, apply, suppress_stored_effort};
use serde_json::json;

/// The ladder rung index `profile` occupies — `cli`, `client`, `profile`, …
const PROFILE_RUNG: usize = 2;

fn caps(supports_reasoning_effort: Option<bool>) -> TemplateCaps {
    TemplateCaps {
        supports_reasoning_effort,
        ..TemplateCaps::default()
    }
}

/// Provenance in which nothing was named. `FieldSources` has no `Default` —
/// the resolver always fills every field — so the cases below build on this.
fn unset() -> FieldSources {
    FieldSources {
        temperature: ParamSource::Unset,
        top_p: ParamSource::Unset,
        top_k: ParamSource::Unset,
        presence_penalty: ParamSource::Unset,
        repeat_penalty: ParamSource::Unset,
        min_p: ParamSource::Unset,
        frequency_penalty: ParamSource::Unset,
        dynatemp_range: ParamSource::Unset,
        dynatemp_exponent: ParamSource::Unset,
        top_n_sigma: ParamSource::Unset,
        dry_multiplier: ParamSource::Unset,
        dry_base: ParamSource::Unset,
        dry_allowed_length: ParamSource::Unset,
        dry_penalty_last_n: ParamSource::Unset,
        max_tokens: ParamSource::Unset,
        reasoning_effort: ParamSource::Unset,
        reasoning_budget_tokens: ParamSource::Unset,
    }
}

/// A resolution where the profile rung named an effort and a budget.
fn stored() -> (InferenceConfig, FieldSources) {
    (
        InferenceConfig {
            reasoning_effort: Some(ReasoningEffort::High),
            reasoning_budget_tokens: Some(16384),
            ..InferenceConfig::default()
        },
        FieldSources {
            reasoning_effort: ParamSource::Layer(PROFILE_RUNG),
            reasoning_budget_tokens: ParamSource::Layer(PROFILE_RUNG),
            ..unset()
        },
    )
}

/// Both record-keeping edits, with no body to delete a key from.
#[test]
fn a_stored_effort_is_suppressed_and_both_records_written() {
    let (mut resolved, mut sources) = stored();

    let suppressed = suppress_stored_effort(&mut resolved, &mut sources, &Some(caps(Some(false))))
        .expect("a positive no suppresses");

    assert_eq!(suppressed.level, ReasoningEffort::High);
    assert_eq!(suppressed.source, ParamSource::Layer(PROFILE_RUNG));
    assert_eq!(resolved.reasoning_effort, None);
    assert_eq!(
        sources.reasoning_effort,
        ParamSource::SuppressedByTemplate,
        "the rung must be replaced, or a surface prints 'profile' for a value \
         that goes nowhere"
    );
}

/// The same asymmetry the live gate has: the budget is enforced by llama.cpp's
/// own sampler rather than by a template, so a template that ignores the effort
/// still honours it. A surface that greyed out both would be wrong about the
/// half that works.
#[test]
fn the_stored_budget_survives_a_template_that_ignores_the_effort() {
    let (mut resolved, mut sources) = stored();
    assert!(
        suppress_stored_effort(&mut resolved, &mut sources, &Some(caps(Some(false)))).is_some()
    );
    assert_eq!(resolved.reasoning_budget_tokens, Some(16384));
    assert_eq!(
        sources.reasoning_budget_tokens,
        ParamSource::Layer(PROFILE_RUNG),
        "the budget's own provenance must be left alone"
    );
}

/// **Unknown never gates, on this side either.** Caps are read from `/props`
/// while a model runs, so most rows in a library have none — and a surface that
/// read that as "not supported" would report every unlaunched model as unable
/// to reason.
#[test]
fn an_unobserved_model_keeps_its_stored_effort() {
    for reading in [None, Some(caps(None)), Some(caps(Some(true)))] {
        let (mut resolved, mut sources) = stored();
        assert_eq!(
            suppress_stored_effort(&mut resolved, &mut sources, &reading),
            None,
            "{reading:?} is not a positive no"
        );
        assert_eq!(resolved.reasoning_effort, Some(ReasoningEffort::High));
        assert_eq!(sources.reasoning_effort, ParamSource::Layer(PROFILE_RUNG));
    }
}

/// Nothing resolved a level, so nothing was going to be sent and there is
/// nothing to report. The ordinary case on a suppressing model too — the gate
/// is not a property of the model alone.
#[test]
fn nothing_to_suppress_reports_nothing() {
    let mut resolved = InferenceConfig::default();
    let mut sources = unset();

    assert_eq!(
        suppress_stored_effort(&mut resolved, &mut sources, &Some(caps(Some(false)))),
        None
    );
    assert_eq!(
        sources.reasoning_effort,
        ParamSource::Unset,
        "an absence nobody asked for stays unset, not suppressed"
    );
}

/// **The reason this is one function and not two.** `gglib model explain` and
/// `GET /api/models/:id/explain` exist to tell an operator what a request would
/// do; a copy of the condition could only ever drift into telling them
/// something else. Every reading of the caps must produce the same verdict on
/// both sides.
#[test]
fn the_live_gate_and_the_stored_gate_agree_on_every_reading() {
    for supports in [None, Some(true), Some(false)] {
        let ctx = ModelContext {
            template_caps: Some(caps(supports)),
            catalog_resolved: true,
            ..ModelContext::passthrough()
        };
        let layers = SamplingLayers {
            profile: Some(InferenceConfig {
                reasoning_effort: Some(ReasoningEffort::High),
                ..InferenceConfig::default()
            }),
            ..SamplingLayers::default()
        };

        let mut body = json!({"model": "m", "messages": []});
        let live = apply(&mut body, &ctx, &layers, None)
            .expect("applies")
            .effort_suppressed;

        let (mut resolved, mut sources) = stored();
        let offline = suppress_stored_effort(&mut resolved, &mut sources, &ctx.template_caps);

        assert_eq!(
            live.map(|s| s.level),
            offline.map(|s| s.level),
            "supports_reasoning_effort={supports:?}"
        );
        assert_eq!(
            body.get("reasoning_effort").is_none(),
            offline.is_some(),
            "the key leaves the body exactly when the offline verdict suppresses"
        );
    }
}
