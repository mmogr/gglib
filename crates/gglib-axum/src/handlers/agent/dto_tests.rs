//! Tests for [`super`] — the agent-chat request DTO's sampling layer.
//!
//! `AgentChatRequest` accepts no sampler parameters and is not about to start;
//! what these pin is that the two reasoning controls are carried, that they are
//! carried *alone*, and that a request naming neither leaves the ladder exactly
//! as it was.

use gglib_core::domain::ReasoningEffort;

use super::{AgentChatRequest, AgentRequestConfig};

fn parse(json: &str) -> AgentChatRequest {
    serde_json::from_str(json).expect("parses")
}

const MINIMAL: &str = r#"{"port":9000,"messages":[]}"#;

#[test]
fn both_reasoning_controls_are_read_from_the_body() {
    let req = parse(
        r#"{"port":9000,"messages":[],"reasoning_effort":"high","reasoning_budget_tokens":16384}"#,
    );

    assert_eq!(req.reasoning_effort, Some(ReasoningEffort::High));
    assert_eq!(req.reasoning_budget_tokens, Some(16384));
}

/// **The layer must stay absent when the request says nothing.** A
/// `Some(InferenceConfig::default())` here would be an empty top rung — inert
/// today, and exactly the shape that starts claiming fields the moment one of
/// them gains a default.
#[test]
fn a_request_naming_neither_control_supplies_no_layer_at_all() {
    assert_eq!(parse(MINIMAL).sampling_layer(), None);
}

/// Either one alone is enough to make a layer. The budget without a level is
/// the ordinary case on a model whose template ignores the level.
#[test]
fn either_control_alone_still_makes_a_layer() {
    let effort = parse(r#"{"port":9000,"messages":[],"reasoning_effort":"low"}"#)
        .sampling_layer()
        .expect("a layer");
    assert_eq!(effort.reasoning_effort, Some(ReasoningEffort::Low));
    assert_eq!(effort.reasoning_budget_tokens, None);

    let budget = parse(r#"{"port":9000,"messages":[],"reasoning_budget_tokens":0}"#)
        .sampling_layer()
        .expect("a layer");
    assert_eq!(budget.reasoning_effort, None);
    assert_eq!(
        budget.reasoning_budget_tokens,
        Some(0),
        "0 means stop thinking, and is not an absence"
    );
}

/// **The door this DTO is not opening.** Every field but the two must stay
/// `None`, or an agent request would start un-tuning the model's own recipe
/// from a layer nobody asked to occupy.
#[test]
fn the_layer_names_the_two_controls_and_nothing_else() {
    let layer = parse(
        r#"{"port":9000,"messages":[],"reasoning_effort":"max","reasoning_budget_tokens":-1}"#,
    )
    .sampling_layer()
    .expect("a layer");

    // Sorted, because the patch is a map and its order is not this test's
    // subject — which keys are in it is.
    let mut named: Vec<String> = layer
        .to_openai_json_patch()
        .into_iter()
        .map(|(key, _)| key)
        .collect();
    named.sort();

    assert_eq!(named, ["reasoning_budget_tokens", "reasoning_effort"]);
}

/// `-1` is upstream's defer sentinel, not a missing value.
#[test]
fn the_defer_sentinel_reaches_the_layer_intact() {
    let layer = parse(r#"{"port":9000,"messages":[],"reasoning_budget_tokens":-1}"#)
        .sampling_layer()
        .expect("a layer");
    assert_eq!(layer.reasoning_budget_tokens, Some(-1));
}

/// No `none` level exists anywhere in gglib: omitting the field is what leaves
/// the template's own default in place, and a second spelling for it would mean
/// something subtly different.
#[test]
fn none_is_refused_as_a_level() {
    assert!(
        serde_json::from_str::<AgentChatRequest>(
            r#"{"port":9000,"messages":[],"reasoning_effort":"none"}"#,
        )
        .is_err()
    );
}

/// The fields are additive: an existing client that never heard of them must
/// keep parsing, and must keep resolving from the layers beneath.
#[test]
fn an_older_client_body_still_parses_and_changes_nothing() {
    let req = parse(r#"{"port":9000,"messages":[],"config":null,"tool_filter":null}"#);

    assert_eq!(req.reasoning_effort, None);
    assert_eq!(req.reasoning_budget_tokens, None);
    assert_eq!(req.sampling_layer(), None);
}

/// Guards the premise of the test above: the loop-tuning DTO beside these
/// fields is untouched by them.
#[test]
fn the_loop_config_is_unaffected() {
    let cfg: AgentRequestConfig = serde_json::from_str(r#"{"max_iterations":3}"#).expect("parses");
    assert_eq!(cfg.max_iterations, Some(3));
}
