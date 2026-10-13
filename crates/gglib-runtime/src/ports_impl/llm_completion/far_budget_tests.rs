//! The reasoning budget a turn for another machine's model carries.
//!
//! That machine's proxy sets a budget of its own on a request with tools that
//! names none (ADR 0019). These pin what
//! [`super::LlmCompletionAdapter::shaped_body`] sends so that it does not.

use super::images::ImageUrls;
use super::*;
use gglib_core::domain::{InferenceConfig, ReasoningEffort};
use serde_json::{Value, json};

const KEY: &str = "reasoning_budget_tokens";

fn far() -> FarMachine {
    FarMachine {
        key: "far-key".to_owned(),
        name: "the desk".to_owned(),
    }
}

fn budget(tokens: i32) -> InferenceConfig {
    InferenceConfig {
        reasoning_budget_tokens: Some(tokens),
        ..InferenceConfig::default()
    }
}

/// What is sent for one user message, to `far_machine` or to this machine,
/// with or without one tool.
fn sent(
    far_machine: Option<FarMachine>,
    sampling: Option<InferenceConfig>,
    with_tool: bool,
) -> Value {
    let tools: Vec<ToolDefinition> = with_tool
        .then(|| ToolDefinition::new("read_file"))
        .into_iter()
        .collect();
    LlmCompletionAdapter::new("http://127.0.0.1:0", Some("m".to_owned()))
        .with_far_machine(far_machine)
        .with_sampling(sampling)
        .shaped_body(
            &[AgentMessage::user("find the bug")],
            &tools,
            &ImageUrls::default(),
            false,
        )
        .unwrap()
}

/// Thinking on is no budget from the caller, and the far proxy would read
/// that as nobody having chosen.
#[test]
fn a_tool_turn_for_another_machine_asks_for_the_launch_default_when_it_names_no_budget() {
    let body = sent(Some(far()), None, true);

    assert_eq!(body["tools"].as_array().map(Vec::len), Some(1), "{body}");
    assert_eq!(body[KEY], json!(-1), "{body}");
}

/// Thinking off is a budget of `0`, and a number the caller typed is that
/// number: neither is replaced.
#[test]
fn a_budget_the_caller_named_goes_to_another_machine_as_named() {
    for named in [0, 2048] {
        let body = sent(Some(far()), Some(budget(named)), true);
        assert_eq!(body[KEY], json!(named), "{body}");
    }
}

/// An effort level is not a budget, so the turn still needs one.
#[test]
fn an_effort_alone_still_gets_the_launch_default_for_another_machine() {
    let effort = InferenceConfig {
        reasoning_effort: Some(ReasoningEffort::High),
        ..InferenceConfig::default()
    };
    let body = sent(Some(far()), Some(effort), true);

    assert_eq!(body[KEY], json!(-1), "{body}");
    assert_eq!(body["reasoning_effort"], json!("high"), "{body}");
}

/// The far proxy's policy reads only a request with tools, so one without
/// them is sent as it always was.
#[test]
fn a_turn_without_tools_for_another_machine_names_no_budget() {
    let body = sent(Some(far()), None, false);

    assert!(body.get("tools").is_none(), "{body}");
    assert!(body.get(KEY).is_none(), "{body}");
}

/// A llama-server on this machine has no policy to opt out of.
#[test]
fn a_tool_turn_for_this_machines_model_names_no_budget() {
    let body = sent(None, None, true);

    assert_eq!(body["tools"].as_array().map(Vec::len), Some(1), "{body}");
    assert!(body.get(KEY).is_none(), "{body}");
}
