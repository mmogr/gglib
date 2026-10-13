//! Tests for when the thinking policy stands down without a resolved budget
//! to defer to: a budget the client sent that could not be read, an effort
//! level somebody chose, and who spoke last as the caller says it.
//!
//! `sampling_budget_tests.rs` holds the four policies and the budgets that
//! stand.

use super::*;
use crate::domain::ReasoningEffort;
use crate::settings::AgenticThinking::{self, Brief, Off, WhenAsked};
use serde_json::json;

const KEY: &str = "reasoning_budget_tokens";
const ALIAS: &str = "thinking_budget_tokens";
const EFFORT: &str = "reasoning_effort";
const MESSAGE_KEY: &str = "reasoning_budget_message";
/// The policies that set a budget on a turn that ends in a tool result.
const ACTING: [AgenticThinking; 3] = [Off, Brief, WhenAsked];

/// A request that carries one tool and ends in a tool result.
fn tool_turn() -> Value {
    json!({
        "tools": [{"function": {"name": "read_file"}}],
        "messages": [
            {"role": "user", "content": "find the bug"},
            {"role": "assistant", "content": null},
            {"role": "tool", "content": "the result"},
        ],
    })
}

fn layers(policy: AgenticThinking) -> SamplingLayers {
    SamplingLayers {
        agentic_adjustments: true,
        agentic_thinking: policy,
        ..Default::default()
    }
}

fn effort() -> InferenceConfig {
    InferenceConfig {
        reasoning_effort: Some(ReasoningEffort::High),
        ..Default::default()
    }
}

fn model(defaults: InferenceConfig, origin: DefaultsOrigin) -> ModelContext {
    ModelContext {
        inference_defaults: Some(defaults),
        defaults_origin: Some(origin),
        ..ModelContext::passthrough()
    }
}

// ── A budget the client sent that could not be read ───────────────────

/// A budget below `-1`, or one that is not a number, is in no rung. The
/// policy still leaves the request as it would be with no policy: under the
/// canonical key the client's own text goes on to llama-server, which answers
/// it, and under the alias the key is taken out as it always is.
#[test]
fn a_budget_the_reader_refused_is_not_replaced_by_the_policys() {
    for sent in [json!(-2), json!("lots")] {
        for key in [KEY, ALIAS] {
            let mut without = tool_turn();
            without[key] = sent.clone();
            let plain = resolve_sampling(
                &mut without,
                &ModelContext::passthrough(),
                &layers(AgenticThinking::Default),
            );
            assert!(
                plain
                    .client_fields_rejected
                    .iter()
                    .any(|issue| issue.field() == key),
                "{key}: {sent} is refused"
            );

            for policy in ACTING {
                let mut body = tool_turn();
                body[key] = sent.clone();
                let decision =
                    resolve_sampling(&mut body, &ModelContext::passthrough(), &layers(policy));

                assert_eq!(body, without, "{policy:?}, {key}: {sent}");
                assert_eq!(decision.agentic_budget_applied, None, "{policy:?} {key}");
                assert_eq!(
                    decision.sources.reasoning_budget_tokens,
                    ParamSource::Unset,
                    "{policy:?} {key}"
                );
                assert_eq!(decision.resolved.reasoning_budget_tokens, None);
                assert!(body.get(MESSAGE_KEY).is_none(), "{body}");
            }
        }
        // What "as it would be with no policy" is, for each spelling.
        assert_eq!(without_policy(KEY, &sent)[KEY], sent);
        let aliased = without_policy(ALIAS, &sent);
        assert!(aliased.get(ALIAS).is_none() && aliased.get(KEY).is_none());
    }
}

/// The tool turn with `key: sent`, shaped under the policy that changes
/// nothing.
fn without_policy(key: &str, sent: &Value) -> Value {
    let mut body = tool_turn();
    body[key] = sent.clone();
    resolve_sampling(
        &mut body,
        &ModelContext::passthrough(),
        &layers(AgenticThinking::Default),
    );
    body
}

// ── An effort level somebody chose ────────────────────────────────────

/// One row per rung a person sets. An effort with no budget beside it is a
/// choice about thinking, so no policy sets a budget over it.
#[test]
fn an_effort_set_by_a_person_stands_the_policy_down_on_every_rung() {
    let trusted = |policy| SamplingLayers {
        trust_client_sampling: true,
        ..layers(policy)
    };
    for policy in ACTING {
        let cases: [(&str, Value, SamplingLayers, ModelContext, usize); 6] = [
            (
                "cli",
                tool_turn(),
                SamplingLayers {
                    cli_override: Some(effort()),
                    ..layers(policy)
                },
                ModelContext::passthrough(),
                0,
            ),
            (
                "client",
                {
                    let mut body = tool_turn();
                    body[EFFORT] = json!("high");
                    body
                },
                trusted(policy),
                ModelContext::passthrough(),
                1,
            ),
            (
                "profile",
                tool_turn(),
                SamplingLayers {
                    profile: Some(effort()),
                    ..layers(policy)
                },
                ModelContext::passthrough(),
                2,
            ),
            (
                "model",
                tool_turn(),
                layers(policy),
                model(effort(), DefaultsOrigin::User),
                3,
            ),
            (
                "global",
                tool_turn(),
                SamplingLayers {
                    global: Some(effort()),
                    ..layers(policy)
                },
                ModelContext::passthrough(),
                4,
            ),
            (
                "model (measured)",
                tool_turn(),
                layers(policy),
                model(effort(), DefaultsOrigin::Measured),
                5,
            ),
        ];

        for (rung, mut body, layers, ctx, index) in cases {
            let decision = resolve_sampling(&mut body, &ctx, &layers);

            assert_eq!(decision.layer_names[index], rung);
            assert_eq!(
                decision.sources.reasoning_effort,
                ParamSource::Layer(index),
                "{rung}"
            );
            assert_eq!(body[EFFORT], json!("high"), "{rung}, {policy:?}");
            assert!(body.get(KEY).is_none(), "{rung}, {policy:?}: {body}");
            assert!(body.get(MESSAGE_KEY).is_none(), "{rung}: {body}");
            assert_eq!(decision.agentic_budget_applied, None, "{rung}, {policy:?}");
            assert_eq!(decision.sources.reasoning_budget_tokens, ParamSource::Unset);
        }
    }
}

/// With client sampling untrusted the ladder discards a client's effort, so
/// nobody chose and the policy sets its budget.
#[test]
fn an_untrusted_clients_effort_is_no_choice() {
    let mut body = tool_turn();
    body[EFFORT] = json!("high");
    let decision = resolve_sampling(&mut body, &ModelContext::passthrough(), &layers(Off));

    assert!(body.get(EFFORT).is_none(), "{body}");
    assert_eq!(decision.client_fields_discarded, [EFFORT]);
    assert_eq!(body[KEY], json!(0), "{body}");
    assert_eq!(decision.agentic_budget_applied, Some(0));
    assert_eq!(
        decision.sources.reasoning_budget_tokens,
        ParamSource::AgenticTurn
    );
}

/// An effort gglib could not read is nobody's choice either, trusted or not.
#[test]
fn an_effort_the_reader_refused_is_no_choice() {
    let mut body = tool_turn();
    body[EFFORT] = json!("banana");
    let trusted = SamplingLayers {
        trust_client_sampling: true,
        ..layers(Off)
    };
    let decision = resolve_sampling(&mut body, &ModelContext::passthrough(), &trusted);

    assert!(body.get(EFFORT).is_none(), "{body}");
    assert_eq!(decision.agentic_budget_applied, Some(0));
}

/// An effort written at import, or published by the model's author, is below
/// global settings and nobody on this machine chose it.
#[test]
fn an_unreviewed_model_rungs_effort_is_no_choice() {
    for origin in [DefaultsOrigin::AutoDetected, DefaultsOrigin::Published] {
        let mut body = tool_turn();
        let decision = resolve_sampling(&mut body, &model(effort(), origin), &layers(Off));

        assert_eq!(body[EFFORT], json!("high"), "{origin:?}");
        assert_eq!(body[KEY], json!(0), "{origin:?}");
        assert_eq!(decision.agentic_budget_applied, Some(0), "{origin:?}");
    }
}

// ── Who spoke last, as the caller says it ─────────────────────────────

/// A caller that changed `messages` before the pipeline says who spoke last
/// in what the client sent, and that is what `when-asked` follows.
#[test]
fn the_callers_last_speaker_outranks_the_bodys() {
    for (role, said, expect) in [
        ("user", LastSpeaker::Work, Some(0)),
        ("tool", LastSpeaker::Person, None),
    ] {
        let mut body = tool_turn();
        body["messages"][2]["role"] = json!(role);
        let layers = SamplingLayers {
            last_speaker: Some(said),
            ..layers(WhenAsked)
        };
        let decision = resolve_sampling(&mut body, &ModelContext::passthrough(), &layers);

        assert_eq!(decision.agentic_budget_applied, expect, "{role}");
    }
}
