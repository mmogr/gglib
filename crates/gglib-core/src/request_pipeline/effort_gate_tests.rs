//! Tests for [`super::suppress_unsupported_effort`], every one named for the
//! failure it prevents.
//!
//! They run through [`apply`] rather than calling the gate directly, because
//! the defect this stage is most exposed to is a **placement** defect: a gate
//! that runs before the sampling fold catches only a client-sent key and lets
//! everything the ladder resolves through. A unit test on the gate alone
//! cannot tell the two placements apart.
//!
//! Split out via `#[path]` so the module itself stays inside the file budget.

use super::*;
use crate::domain::{InferenceConfig, TemplateCaps};
use crate::request_pipeline::{SamplingLayers, apply};
use serde_json::json;

/// The ladder rung index `profile` occupies — `cli`, `client`, `profile`, …
/// See `resolve_sampling`'s `ordered` array.
const PROFILE_RUNG: usize = 2;

fn caps(supports_reasoning_effort: Option<bool>) -> TemplateCaps {
    TemplateCaps {
        supports_reasoning_effort,
        ..TemplateCaps::default()
    }
}

/// A resolved catalog row whose launch reported these caps.
fn observed(supports_reasoning_effort: Option<bool>) -> ModelContext {
    ModelContext {
        template_caps: Some(caps(supports_reasoning_effort)),
        catalog_resolved: true,
        ..ModelContext::passthrough()
    }
}

/// A resolved catalog row nobody has launched yet — the tri-state's "never
/// observed", which is `None` on the row rather than a caps object of `None`s.
fn never_observed() -> ModelContext {
    ModelContext {
        catalog_resolved: true,
        ..ModelContext::passthrough()
    }
}

fn body() -> Value {
    json!({"model": "m", "messages": [{"role": "user", "content": "hi"}]})
}

/// A body carrying the client's own level, and a trusting gate above it —
/// the only way a client's `reasoning_effort` becomes a resolved value.
fn trusted_client(level: &str) -> (Value, SamplingLayers) {
    let mut body = body();
    body["reasoning_effort"] = json!(level);
    (
        body,
        SamplingLayers {
            trust_client_sampling: true,
            ..SamplingLayers::default()
        },
    )
}

/// The ladder — not the client — asking for a level, which is the case the
/// stage's placement exists for.
fn profile_asking_for(config: InferenceConfig) -> SamplingLayers {
    SamplingLayers {
        profile: Some(config),
        ..SamplingLayers::default()
    }
}

fn effort(level: ReasoningEffort) -> InferenceConfig {
    InferenceConfig {
        reasoning_effort: Some(level),
        ..InferenceConfig::default()
    }
}

#[track_caller]
fn assert_effort_on_the_wire(body: &Value, expected: &str) {
    assert_eq!(
        body.get(REASONING_EFFORT_KEY).and_then(Value::as_str),
        Some(expected),
        "the level must reach llama-server: {body}"
    );
}

#[track_caller]
fn assert_no_effort_on_the_wire(body: &Value) {
    assert!(
        body.get(REASONING_EFFORT_KEY).is_none(),
        "a suppressed level must not reach llama-server: {body}"
    );
}

/// The case the stage exists for.
#[test]
fn an_effort_never_reaches_a_model_whose_template_ignores_it() {
    let (mut body, layers) = trusted_client("high");
    let report = apply(&mut body, &observed(Some(false)), &layers, None).expect("pipeline applies");

    assert_no_effort_on_the_wire(&body);
    assert_eq!(
        report.sampling.resolved.reasoning_effort, None,
        "the decision must stop claiming a value it did not send"
    );
    assert_eq!(
        report.effort_suppressed.map(|s| s.level),
        Some(ReasoningEffort::High)
    );
}

/// The other half of the same model: a template that does read the variable
/// gets the level, so the gate is a gate and not a deletion.
#[test]
fn a_model_whose_template_reads_it_gets_its_effort() {
    let (mut body, layers) = trusted_client("high");
    let report = apply(&mut body, &observed(Some(true)), &layers, None).expect("pipeline applies");

    assert_effort_on_the_wire(&body, "high");
    assert!(report.effort_suppressed.is_none());
}

/// Never observed is not "not supported". A model nobody has launched has no
/// caps row at all, and ADR 0007 decision 3 licenses nothing from that.
#[test]
fn an_unobserved_model_keeps_its_effort() {
    let (mut body, layers) = trusted_client("high");
    let report = apply(&mut body, &never_observed(), &layers, None).expect("pipeline applies");

    assert_effort_on_the_wire(&body, "high");
    assert!(report.effort_suppressed.is_none());
}

/// A caps read that arrived without this field is also unknown — five of the
/// nine bools default `true` upstream, so an absent key licenses no
/// conclusion in either direction.
#[test]
fn a_caps_read_that_omits_the_field_keeps_its_effort() {
    let (mut body, layers) = trusted_client("high");
    let report = apply(&mut body, &observed(None), &layers, None).expect("pipeline applies");

    assert_effort_on_the_wire(&body, "high");
    assert!(report.effort_suppressed.is_none());
}

/// The `catalog_resolved` conjunct, pinned on its own: caps that somehow sit
/// on a passthrough context are still not a fact about *this* request's
/// model, and the same discipline `strip_unsupported_tools` applies to an
/// empty capability bitfield applies here.
#[test]
fn an_unresolved_model_keeps_its_effort() {
    let (mut body, layers) = trusted_client("high");
    let ctx = ModelContext {
        template_caps: Some(caps(Some(false))),
        catalog_resolved: false,
        ..ModelContext::passthrough()
    };
    let report = apply(&mut body, &ctx, &layers, None).expect("pipeline applies");

    assert_effort_on_the_wire(&body, "high");
    assert!(report.effort_suppressed.is_none());
}

/// **The regression test for the placement bug.** The client sends no level;
/// a `:high` profile supplies one, and the untrusted default gate is on — so
/// there is nothing in the body for a stage-2b gate to strip, and the value
/// appears only when `resolve_sampling` force-inserts it at stage 4. Move
/// this stage before the fold and the assertion below fails while every
/// client-sent case still passes.
#[test]
fn a_ladder_supplied_effort_is_suppressed_not_just_a_client_one() {
    let mut body = body();
    let layers = profile_asking_for(effort(ReasoningEffort::High));
    let report = apply(&mut body, &observed(Some(false)), &layers, None).expect("pipeline applies");

    assert!(
        body.get(REASONING_EFFORT_KEY).is_none(),
        "the profile's level was force-inserted past a gate that ran too early: {body}"
    );
    assert_eq!(
        report.effort_suppressed,
        Some(SuppressedEffort {
            level: ReasoningEffort::High,
            source: ParamSource::Layer(PROFILE_RUNG),
        }),
        "the record must name the rung that asked, not just the level"
    );
}

/// The budget is not a template variable: llama.cpp's own sampler enforces it
/// and upstream range-validates it, so a template that cannot read
/// `reasoning_effort` honours a budget exactly as any other model does.
/// Gating it here would be a category error.
#[test]
fn the_budget_survives_a_model_that_cannot_honour_effort() {
    let mut body = body();
    let layers = profile_asking_for(InferenceConfig {
        reasoning_effort: Some(ReasoningEffort::High),
        reasoning_budget_tokens: Some(256),
        ..InferenceConfig::default()
    });
    let report = apply(&mut body, &observed(Some(false)), &layers, None).expect("pipeline applies");

    assert_no_effort_on_the_wire(&body);
    assert_eq!(
        body.get("reasoning_budget_tokens").and_then(Value::as_i64),
        Some(256),
        "the budget is sampler-enforced and template-independent: {body}"
    );
    assert_eq!(
        report.sampling.resolved.reasoning_budget_tokens,
        Some(256),
        "and the decision must still say so"
    );
}

/// A value that vanishes without a record is the defect this whole arc
/// exists to prevent, and no readback will ever catch it — neither reasoning
/// control is echoed anywhere (ADR 0007 finding 7a). So the provenance has to
/// carry it.
#[test]
fn a_suppressed_effort_is_named_in_provenance() {
    let mut body = body();
    let layers = profile_asking_for(effort(ReasoningEffort::Max));
    let report = apply(&mut body, &observed(Some(false)), &layers, None).expect("pipeline applies");

    let decision = &report.sampling;
    assert_eq!(
        decision.sources.reasoning_effort,
        ParamSource::SuppressedByTemplate,
        "provenance must not go on naming a rung whose value was dropped"
    );
    assert!(
        decision
            .sources
            .describe(&decision.layer_names)
            .contains("reasoning_effort=suppressed-by-template"),
        "the rendered provenance line is where an operator reads it"
    );
    assert_eq!(
        report.effort_suppressed.map(|s| s.level),
        Some(ReasoningEffort::Max),
        "and the level that was dropped is part of the account"
    );
}

/// Nothing resolved, nothing suppressed: the ordinary request on an ignoring
/// model reports no suppression, so a surface can treat `Some` as "something
/// was actually thrown away".
#[test]
fn a_request_with_no_effort_reports_no_suppression() {
    let mut body = body();
    let report = apply(
        &mut body,
        &observed(Some(false)),
        &SamplingLayers::default(),
        None,
    )
    .expect("pipeline applies");

    assert!(report.effort_suppressed.is_none());
    assert_eq!(
        report.sampling.sources.reasoning_effort,
        ParamSource::Unset,
        "an absence nobody asked for is unset, not suppressed"
    );
}
