//! Tests for the thinking policy: the reasoning budget [`super::resolve_sampling`]
//! sets on a turn that carries tools.
//!
//! Their own file because they are one behaviour, and `sampling_tests.rs`
//! holds the ladder and the temperature ceiling.

use super::*;
use crate::settings::AgenticThinking::{self, Brief, Off, WhenAsked};
use serde_json::json;

const KEY: &str = "reasoning_budget_tokens";
const MESSAGE_KEY: &str = "reasoning_budget_message";
const EVERY_POLICY: [AgenticThinking; 4] = [AgenticThinking::Default, Off, Brief, WhenAsked];

/// A request that carries one tool and whose last message has `last_role`.
fn tool_turn(last_role: &str) -> Value {
    json!({
        "tools": [{"function": {"name": "read_file"}}],
        "messages": [
            {"role": "user", "content": "find the bug"},
            {"role": "assistant", "content": null},
            {"role": last_role, "content": "the last message"},
        ],
    })
}

/// The proxy's layers under `policy`, with the agentic adjustments on.
fn layers(policy: AgenticThinking) -> SamplingLayers {
    SamplingLayers {
        agentic_adjustments: true,
        agentic_thinking: policy,
        ..Default::default()
    }
}

fn budget(tokens: i32) -> InferenceConfig {
    InferenceConfig {
        reasoning_budget_tokens: Some(tokens),
        ..Default::default()
    }
}

/// A model whose stored defaults are `defaults`, written by `origin`.
fn model(defaults: InferenceConfig, origin: DefaultsOrigin) -> ModelContext {
    ModelContext {
        inference_defaults: Some(defaults),
        defaults_origin: Some(origin),
        ..ModelContext::passthrough()
    }
}

fn resolve(body: &mut Value, policy: AgenticThinking) -> SamplingDecision {
    resolve_sampling(body, &ModelContext::passthrough(), &layers(policy))
}

/// The policy set `tokens`: in the body, in the decision and in the
/// provenance, which names no rung.
#[track_caller]
fn assert_set(body: &Value, decision: &SamplingDecision, tokens: i32) {
    assert_eq!(body[KEY], json!(tokens), "{body}");
    assert_eq!(decision.agentic_budget_applied, Some(tokens));
    assert_eq!(decision.resolved.reasoning_budget_tokens, Some(tokens));
    assert_eq!(
        decision.sources.reasoning_budget_tokens,
        ParamSource::AgenticTurn
    );
}

/// The policy set nothing, and the body carries no budget and no message.
#[track_caller]
fn assert_untouched(body: &Value, decision: &SamplingDecision) {
    assert!(body.get(KEY).is_none(), "{body}");
    assert!(body.get(MESSAGE_KEY).is_none(), "{body}");
    assert_eq!(decision.agentic_budget_applied, None);
    assert_eq!(decision.resolved.reasoning_budget_tokens, None);
    assert_eq!(decision.sources.reasoning_budget_tokens, ParamSource::Unset);
}

// ── The four policies ─────────────────────────────────────────────────

#[test]
fn when_asked_stops_thinking_after_a_tool_result_and_sends_no_message() {
    for last_role in ["tool", "assistant", "system"] {
        let mut body = tool_turn(last_role);
        let decision = resolve(&mut body, WhenAsked);

        assert_set(&body, &decision, 0);
        assert!(body.get(MESSAGE_KEY).is_none(), "{last_role}: {body}");
    }
}

#[test]
fn when_asked_leaves_a_turn_alone_when_the_user_spoke_last() {
    let mut body = tool_turn("user");
    let decision = resolve(&mut body, WhenAsked);

    assert_untouched(&body, &decision);
    assert!(decision.agentic_turn, "the turn is still an agentic one");
}

#[test]
fn off_stops_thinking_whoever_spoke_last() {
    for last_role in ["tool", "user"] {
        let mut body = tool_turn(last_role);
        let decision = resolve(&mut body, Off);

        assert_set(&body, &decision, 0);
        assert!(body.get(MESSAGE_KEY).is_none(), "{last_role}: {body}");
    }
}

#[test]
fn brief_sends_512_tokens_and_the_message_whoever_spoke_last() {
    for last_role in ["tool", "user"] {
        let mut body = tool_turn(last_role);
        let decision = resolve(&mut body, Brief);

        assert_set(&body, &decision, 512);
        assert_eq!(
            body[MESSAGE_KEY],
            json!("Enough thinking. Act now."),
            "{last_role}"
        );
    }
}

#[test]
fn brief_keeps_a_message_the_client_sent() {
    let mut body = tool_turn("tool");
    body[MESSAGE_KEY] = json!("the client's own words");
    let decision = resolve(&mut body, Brief);

    assert_set(&body, &decision, 512);
    assert_eq!(body[MESSAGE_KEY], json!("the client's own words"));
}

#[test]
fn the_default_policy_changes_nothing() {
    for last_role in ["tool", "user"] {
        let mut body = tool_turn(last_role);
        let decision = resolve(&mut body, AgenticThinking::Default);

        assert_untouched(&body, &decision);
    }
}

/// `SamplingLayers::default()` is what a caller with no settings passes.
#[test]
fn bare_default_layers_carry_the_policy_that_changes_nothing() {
    assert_eq!(
        SamplingLayers::default().agentic_thinking,
        AgenticThinking::Default
    );
}

// ── Where it does not reach ───────────────────────────────────────────

#[test]
fn a_turn_without_tools_is_untouched_under_every_policy() {
    for policy in EVERY_POLICY {
        for last_role in ["tool", "user"] {
            let mut body = tool_turn(last_role);
            body.as_object_mut().unwrap().remove("tools");
            let decision = resolve(&mut body, policy);

            assert_untouched(&body, &decision);
        }
    }
}

/// The switch the temperature ceiling reads gates the budget too: with the
/// adjustments off, no policy sets anything.
#[test]
fn no_policy_acts_while_the_agentic_adjustments_are_off() {
    for policy in EVERY_POLICY {
        let mut body = tool_turn("tool");
        let decision = resolve_sampling(
            &mut body,
            &ModelContext::passthrough(),
            &SamplingLayers {
                agentic_adjustments: false,
                agentic_thinking: policy,
                ..Default::default()
            },
        );

        assert_untouched(&body, &decision);
    }
}

// ── A budget somebody chose stands ────────────────────────────────────

/// `0`, a number and `-1` (the launch default) are all the client's own, and
/// the client is honoured whether or not its sampling is trusted.
#[test]
fn a_clients_own_budget_stands_under_every_policy() {
    for policy in EVERY_POLICY {
        for sent in [0, 2048, -1] {
            for trusted in [false, true] {
                let mut body = tool_turn("tool");
                body[KEY] = json!(sent);
                let decision = resolve_sampling(
                    &mut body,
                    &ModelContext::passthrough(),
                    &SamplingLayers {
                        trust_client_sampling: trusted,
                        ..layers(policy)
                    },
                );

                assert_eq!(body[KEY], json!(sent), "{policy:?}, trusted {trusted}");
                assert_eq!(decision.agentic_budget_applied, None, "{policy:?} {sent}");
                assert_eq!(
                    decision.sources.reasoning_budget_tokens,
                    ParamSource::Layer(1),
                    "the client rung, {policy:?} {sent}"
                );
                assert!(body.get(MESSAGE_KEY).is_none(), "{body}");
            }
        }
    }
}

/// llama-server's second spelling is read as the same budget.
#[test]
fn a_clients_budget_under_the_alias_stands() {
    let mut body = tool_turn("tool");
    body["thinking_budget_tokens"] = json!(300);
    let decision = resolve(&mut body, Off);

    assert_eq!(body[KEY], json!(300));
    assert_eq!(decision.agentic_budget_applied, None);
}

/// One row per rung a person sets: the flag, the profile, the model's own
/// defaults and the global defaults.
#[test]
fn a_budget_set_by_a_person_stands_on_every_rung() {
    let chosen = budget(1500);
    let cases: [(&str, SamplingLayers, ModelContext, usize); 4] = [
        (
            "cli",
            SamplingLayers {
                cli_override: Some(chosen.clone()),
                ..layers(Off)
            },
            ModelContext::passthrough(),
            0,
        ),
        (
            "profile",
            SamplingLayers {
                profile: Some(chosen.clone()),
                ..layers(Off)
            },
            ModelContext::passthrough(),
            2,
        ),
        (
            "model",
            layers(Off),
            model(chosen.clone(), DefaultsOrigin::User),
            3,
        ),
        (
            "global",
            SamplingLayers {
                global: Some(chosen),
                ..layers(Off)
            },
            ModelContext::passthrough(),
            4,
        ),
    ];

    for (rung, layers, ctx, index) in cases {
        let mut body = tool_turn("tool");
        let decision = resolve_sampling(&mut body, &ctx, &layers);

        assert_eq!(body[KEY], json!(1500), "{rung}");
        assert_eq!(decision.agentic_budget_applied, None, "{rung}");
        assert_eq!(decision.layer_names[index], rung);
        assert_eq!(
            decision.sources.reasoning_budget_tokens,
            ParamSource::Layer(index),
            "{rung}"
        );
    }
}

// ── A budget nobody chose does not ────────────────────────────────────

/// A recipe written at import and one a model's author published are both
/// below global settings, and nobody on this machine chose either.
#[test]
fn an_unreviewed_model_rungs_budget_is_replaced() {
    for origin in [DefaultsOrigin::AutoDetected, DefaultsOrigin::Published] {
        let mut body = tool_turn("tool");
        let decision = resolve_sampling(&mut body, &model(budget(4096), origin), &layers(Off));

        assert_set(&body, &decision, 0);
    }
}

#[test]
fn a_measured_model_rungs_budget_stands() {
    let mut body = tool_turn("tool");
    let ctx = model(budget(4096), DefaultsOrigin::Measured);
    let decision = resolve_sampling(&mut body, &ctx, &layers(Off));

    assert_eq!(body[KEY], json!(4096));
    assert_eq!(decision.agentic_budget_applied, None);
    assert_eq!(
        decision.sources.reasoning_budget_tokens,
        ParamSource::Layer(LADDER_RUNGS - 1)
    );
}

/// Only the measured rung is exempt: a measured recipe that names no budget
/// measured none.
#[test]
fn a_measured_model_that_names_no_budget_still_gets_the_policys() {
    let mut body = tool_turn("tool");
    let recipe = InferenceConfig {
        top_k: Some(20),
        ..Default::default()
    };
    let ctx = model(recipe, DefaultsOrigin::Measured);
    let decision = resolve_sampling(&mut body, &ctx, &layers(Off));

    assert_set(&body, &decision, 0);
}

// ── Any model ─────────────────────────────────────────────────────────

/// The `reasoning` tag is not asked: a model without it, a model with it and
/// a model gglib knows nothing about all get the budget.
#[test]
fn the_budget_is_set_whatever_the_model_is_tagged() {
    let tagged = |tags: &[&str]| ModelContext {
        tags: tags.iter().map(|&tag| tag.to_owned()).collect(),
        ..ModelContext::passthrough()
    };
    for ctx in [
        tagged(&[]),
        tagged(&["reasoning"]),
        ModelContext::passthrough(),
    ] {
        let mut body = tool_turn("tool");
        let decision = resolve_sampling(&mut body, &ctx, &layers(WhenAsked));

        assert_set(&body, &decision, 0);
    }
}

/// The two agentic adjustments are separate: on a model that has a ceiling,
/// a tool turn gets both, and the temperature keeps the rung it came from.
#[test]
fn the_budget_and_the_temperature_ceiling_apply_side_by_side() {
    let mut body = tool_turn("tool");
    let recipe = InferenceConfig {
        temperature: Some(1.0),
        ..Default::default()
    };
    let ctx = model(recipe, DefaultsOrigin::AutoDetected);
    let decision = resolve_sampling(&mut body, &ctx, &layers(WhenAsked));

    assert_set(&body, &decision, 0);
    assert_eq!(decision.agentic_ceiling_applied, Some(0.3));
    assert_eq!(
        decision.sources.temperature,
        ParamSource::Layer(LADDER_RUNGS - 1)
    );
}

/// The debug line and the audit print the provenance through `describe`.
#[test]
fn the_provenance_line_names_the_policy_and_no_rung() {
    let mut body = tool_turn("tool");
    let decision = resolve(&mut body, WhenAsked);
    let line = decision.sources.describe(&decision.layer_names);

    assert!(
        line.ends_with("reasoning_budget_tokens=agentic-turn"),
        "{line}"
    );
}

// ── The environment switch ────────────────────────────────────────────

/// Tells [`the_probe_a_child_process_runs`] what to expect. Absent in an
/// ordinary run.
const PROBE_ENV: &str = "GGLIB_TEST_AGENTIC_BUDGET_PROBE";

/// Run in a child process by the test below, with the kill switch set or
/// removed in that process's environment; a pass-through in an ordinary run.
#[test]
fn the_probe_a_child_process_runs() {
    let Ok(expected) = std::env::var(PROBE_ENV) else {
        return;
    };
    let mut body = tool_turn("tool");
    let decision = resolve(&mut body, Off);
    match expected.as_str() {
        "untouched" => assert_untouched(&body, &decision),
        "set" => assert_set(&body, &decision, 0),
        other => panic!("unknown probe expectation {other}"),
    }
}

/// `GGLIB_DISABLE_AGENTIC_SAMPLING` switches the policy off with the ceiling.
///
/// The variable is the process's, and every other test here resolves in this
/// one, so the probe runs in a child: this test binary again, filtered to the
/// probe, with the variable truthy, not truthy, and absent.
#[test]
fn the_environment_switch_turns_the_policy_off() {
    let probe = "request_pipeline::sampling::sampling_budget_tests::the_probe_a_child_process_runs";
    for (switch, expected) in [(Some("1"), "untouched"), (Some("0"), "set"), (None, "set")] {
        let mut child = std::process::Command::new(std::env::current_exe().expect("this binary"));
        child
            .args([probe, "--exact", "--test-threads=1"])
            .env(PROBE_ENV, expected)
            .env_remove(DISABLE_AGENTIC_SAMPLING_ENV);
        if let Some(value) = switch {
            child.env(DISABLE_AGENTIC_SAMPLING_ENV, value);
        }
        let out = child.output().expect("running the probe");
        let stdout = String::from_utf8_lossy(&out.stdout);

        assert!(
            out.status.success() && stdout.contains("1 passed"),
            "switch {switch:?} must leave the budget {expected}\n{stdout}\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}
