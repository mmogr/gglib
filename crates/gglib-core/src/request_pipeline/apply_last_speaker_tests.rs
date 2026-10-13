//! Stage 0 of [`super::apply`]: who spoke last is read from the request as the
//! client sent it, before stage 2 can change the role of its last message.

use super::*;
use crate::domain::ModelCapabilities;
use crate::settings::AgenticThinking;
use serde_json::json;

/// A turn with one tool that ends in a `system` message after a tool result.
fn ends_in_a_system_reminder() -> Value {
    json!({
        "tools": [{"type": "function", "function": {"name": "read_file"}}],
        "messages": [
            {"role": "user", "content": "find the bug"},
            {"role": "assistant", "content": "", "tool_calls": [{
                "id": "c1", "type": "function",
                "function": {"name": "read_file", "arguments": "{}"},
            }]},
            {"role": "tool", "tool_call_id": "c1", "content": "the file"},
            {"role": "system", "content": "reminder"},
        ],
    })
}

fn when_asked() -> SamplingLayers {
    SamplingLayers {
        agentic_adjustments: true,
        agentic_thinking: AgenticThinking::WhenAsked,
        ..SamplingLayers::default()
    }
}

/// A catalogued model that calls tools, with `more` beside that.
fn model(more: ModelCapabilities) -> ModelContext {
    ModelContext {
        catalog_resolved: true,
        capabilities: ModelCapabilities::SUPPORTS_TOOL_CALLS | more,
        ..ModelContext::passthrough()
    }
}

/// On a model with no system role stage 2 sends the reminder as a `user`
/// message. The person did not write it, so `when-asked` sets its budget of
/// `0` there as it does on a model that takes the reminder as sent.
#[test]
fn a_system_message_rewritten_for_the_model_is_not_the_person_speaking() {
    let last_role_sent = |ctx: &ModelContext| {
        let mut body = ends_in_a_system_reminder();
        let report = apply(&mut body, ctx, &when_asked(), None).unwrap();
        let last = body["messages"].as_array().unwrap().last().unwrap().clone();
        (
            last["role"].clone(),
            report.sampling.agentic_budget_applied,
            body["reasoning_budget_tokens"].clone(),
        )
    };

    let with_system_role = last_role_sent(&model(ModelCapabilities::SUPPORTS_SYSTEM_ROLE));
    let without = last_role_sent(&model(ModelCapabilities::empty()));

    assert_eq!(with_system_role, (json!("system"), Some(0), json!(0)));
    assert_eq!(without, (json!("user"), Some(0), json!(0)));
}

/// A caller that added a message of its own says who spoke last before it
/// did, and the pipeline follows that and not the body it was handed.
#[test]
fn the_callers_last_speaker_is_followed_through_the_pipeline() {
    let mut body = ends_in_a_system_reminder();
    body["messages"]
        .as_array_mut()
        .unwrap()
        .push(json!({"role": "user", "content": "a note the caller added"}));
    let mut unsaid = body.clone();
    let said = SamplingLayers {
        last_speaker: Some(LastSpeaker::Work),
        ..when_asked()
    };
    let ctx = ModelContext::passthrough();

    let report = apply(&mut body, &ctx, &said, None).unwrap();
    assert_eq!(report.sampling.agentic_budget_applied, Some(0));

    let report = apply(&mut unsaid, &ctx, &when_asked(), None).unwrap();
    assert_eq!(report.sampling.agentic_budget_applied, None);
}
