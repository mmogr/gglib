//! Tests for [`super`]: a turn's sampling layer, which carries its thinking
//! budget and its chat's saved sampling and nothing else, and its limits,
//! resolved against the stored ones.

use super::super::chat_sampling::ChatSampling;
use super::{LoopRequest, NamedLimits};

/// **The layer must stay absent when the turn says nothing.** A
/// `Some(InferenceConfig::default())` here would be an empty top rung — inert
/// today, and exactly the shape that starts claiming fields the moment one of
/// them gains a default.
#[test]
fn a_turn_naming_nothing_supplies_no_layer_at_all() {
    assert_eq!(LoopRequest::default().sampling_layer(), None);
}

/// Thinking off is a layer of its own: `0` means stop thinking, and is not
/// an absence. It names that budget and nothing else.
#[test]
fn thinking_off_alone_makes_a_layer_naming_only_the_budget() {
    let off = LoopRequest {
        reasoning_budget_tokens: Some(0),
        ..LoopRequest::default()
    };
    let layer = off.sampling_layer().expect("a layer");
    let named: Vec<String> = layer
        .to_openai_json_patch()
        .into_iter()
        .map(|(key, _)| key)
        .collect();
    assert_eq!(named, ["reasoning_budget_tokens"]);
    assert_eq!(layer.reasoning_budget_tokens, Some(0));
}

/// A chat's saved sampling makes a layer without a budget, carrying the
/// saved values as a client's.
#[test]
fn a_chats_saved_sampling_alone_makes_a_layer() {
    let settings = gglib_core::domain::chat::ConversationSettings {
        temperature: Some(0.3),
        ..Default::default()
    };
    let saved = LoopRequest {
        saved: ChatSampling::of(&settings),
        ..LoopRequest::default()
    };
    let layer = saved.sampling_layer().expect("a layer");
    assert_eq!(layer.temperature, Some(0.3));
    assert_eq!(layer.reasoning_budget_tokens, None);
}

/// The tool limits Settings › Tools stores reach a turn that names none: a
/// turn on a chat that saved none of its own, from any door.
#[test]
fn a_turn_naming_no_tool_limit_runs_with_the_stored_ones() {
    let settings = gglib_core::Settings {
        max_parallel_tools: Some(4),
        tool_timeout_ms: Some(5_000),
        ..gglib_core::Settings::default()
    };
    let config = NamedLimits::default().into_agent_config(Some(&settings));
    assert_eq!(config.max_parallel_tools, 4);
    assert_eq!(config.tool_timeout_ms, 5_000);
}

/// A chat's own limit, which `hub_turn` names on its turn, outranks the
/// stored one; a limit it leaves out is still the stored one.
#[test]
fn a_chats_own_tool_limit_outranks_the_stored_one() {
    let settings = gglib_core::Settings {
        max_parallel_tools: Some(4),
        tool_timeout_ms: Some(5_000),
        ..gglib_core::Settings::default()
    };
    let named = NamedLimits {
        max_parallel_tools: Some(2),
        ..NamedLimits::default()
    };
    let config = named.into_agent_config(Some(&settings));
    assert_eq!(config.max_parallel_tools, 2);
    assert_eq!(config.tool_timeout_ms, 5_000);
}
