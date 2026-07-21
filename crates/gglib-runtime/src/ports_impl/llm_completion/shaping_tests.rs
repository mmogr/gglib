//! What the adapter actually puts on the wire.
//!
//! [`super::LlmCompletionAdapter::shaped_body`] is the whole pre-transport
//! path: translation in `body`, then the shared pipeline. The rules themselves
//! are tested in `gglib_core::request_pipeline`; these assert that the agent
//! path is genuinely wired to them, which is the thing that had drifted.

use super::*;
use gglib_core::domain::agent::{AssistantContent, ToolCall};
use gglib_core::domain::{InferenceConfig, ModelCapabilities};
use serde_json::{Value, json};

fn adapter(model_context: ModelContext, sampling: Option<InferenceConfig>) -> LlmCompletionAdapter {
    LlmCompletionAdapter::new("http://127.0.0.1:0", Some("m".to_owned()))
        .with_model_context(model_context)
        .with_sampling(sampling)
}

fn strict_turns() -> ModelContext {
    ModelContext {
        capabilities: ModelCapabilities::REQUIRES_STRICT_TURNS,
        ..ModelContext::passthrough()
    }
}

fn user(text: &str) -> AgentMessage {
    AgentMessage::User {
        content: text.to_owned(),
    }
}

fn body_of(adapter: &LlmCompletionAdapter, messages: &[AgentMessage]) -> Value {
    adapter.shaped_body(messages, &[], None).unwrap()
}

// ── Stage 1: reasoning strip ──────────────────────────────────────────────

/// Moved out of `build_chat_body` into the shared pipeline; it must still run.
#[test]
fn prior_turn_reasoning_is_stripped() {
    let messages = vec![
        user("hi"),
        AgentMessage::Assistant {
            content: AssistantContent {
                text: Some("<think>ramble</think>answer".to_owned()),
                tool_calls: vec![],
            },
        },
    ];
    let body = body_of(&adapter(ModelContext::passthrough(), None), &messages);
    assert_eq!(body["messages"][1]["content"], "answer");
}

// ── Stage 2: capability coalescing ────────────────────────────────────────

/// The capability the agent path never had. A strict-turn model receiving
/// consecutive same-role messages raises a hard 500 from its Jinja template.
#[test]
fn strict_turn_models_get_consecutive_messages_merged() {
    let body = body_of(
        &adapter(strict_turns(), None),
        &[user("one"), user("two"), user("three")],
    );

    let messages = body["messages"].as_array().expect("messages array");
    assert_eq!(messages.len(), 1, "three user turns merge into one");
    assert_eq!(messages[0]["content"], "one\n\ntwo\n\nthree");
}

/// Coalescing must not cost the agent loop its tool wiring — the merge runs
/// through a typed round-trip, and `tool_call_id` is not one of the fields it
/// models.
#[test]
fn coalescing_preserves_tool_call_ids() {
    let messages = vec![
        user("run it"),
        AgentMessage::Assistant {
            content: AssistantContent {
                text: None,
                tool_calls: vec![ToolCall {
                    id: "call_1".to_owned(),
                    name: "f".to_owned(),
                    arguments: json!({}),
                }],
            },
        },
        AgentMessage::Tool {
            tool_call_id: "call_1".to_owned(),
            content: "result".to_owned(),
        },
    ];
    let body = body_of(&adapter(strict_turns(), None), &messages);

    let tool = body["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["role"] == "tool")
        .expect("tool message survives");
    assert_eq!(tool["tool_call_id"], "call_1");
    assert_eq!(tool["content"], "result");
}

/// A model the catalog could not resolve must lose its model-specific
/// handling and nothing else.
#[test]
fn a_passthrough_context_merges_nothing() {
    let body = body_of(
        &adapter(ModelContext::passthrough(), None),
        &[user("one"), user("two")],
    );
    assert_eq!(body["messages"].as_array().unwrap().len(), 2);
}

// ── Stages 4–5: sampling and cache_prompt ─────────────────────────────────

/// The per-model layer the agent path never applied.
#[test]
fn per_model_inference_defaults_reach_the_body() {
    let ctx = ModelContext {
        inference_defaults: Some(InferenceConfig {
            temperature: Some(0.42),
            ..Default::default()
        }),
        ..ModelContext::passthrough()
    };
    let body = body_of(&adapter(ctx, None), &[user("hi")]);
    assert!((body["temperature"].as_f64().unwrap() - 0.42).abs() < 1e-6);
}

/// The caller's own parameters are the top layer: they beat the model's
/// stored defaults, and the model still fills in what they left unset.
#[test]
fn caller_sampling_beats_model_defaults_without_erasing_them() {
    let ctx = ModelContext {
        inference_defaults: Some(InferenceConfig {
            temperature: Some(0.42),
            presence_penalty: Some(1.5),
            ..Default::default()
        }),
        ..ModelContext::passthrough()
    };
    let caller = InferenceConfig {
        temperature: Some(0.11),
        ..Default::default()
    };
    let body = body_of(&adapter(ctx, Some(caller)), &[user("hi")]);

    assert!((body["temperature"].as_f64().unwrap() - 0.11).abs() < 1e-6);
    assert!((body["presence_penalty"].as_f64().unwrap() - 1.5).abs() < 1e-6);
}

/// Pinned here for the same reason it is pinned in the proxy: the agent path
/// benefits from the same KV reuse, and the two bodies should not differ.
#[test]
fn cache_prompt_is_pinned() {
    let body = body_of(&adapter(ModelContext::passthrough(), None), &[user("hi")]);
    assert_eq!(body["cache_prompt"], true);
}

// ── Stage 3: history truncation ───────────────────────────────────────────

/// Sized so the leading tool results sit outside the protected tail.
fn long_conversation(tool_result_chars: usize) -> Vec<AgentMessage> {
    let mut messages = vec![
        AgentMessage::Tool {
            tool_call_id: "call_1".to_owned(),
            content: "x".repeat(tool_result_chars),
        },
        AgentMessage::Tool {
            tool_call_id: "call_2".to_owned(),
            content: "x".repeat(tool_result_chars),
        },
    ];
    for _ in 0..8 {
        messages.push(user("ok"));
    }
    messages
}

fn ctx_with_context_length(tokens: u64) -> ModelContext {
    ModelContext {
        context_length: Some(tokens),
        ..ModelContext::passthrough()
    }
}

/// The thing the agent path never had: an oversized conversation is trimmed
/// before it is sent, rather than being handed whole to llama-server.
#[test]
fn an_oversized_conversation_is_truncated_on_the_agent_path() {
    // 4096 tokens ≈ a 16,384-char budget; ~60k chars of tool output.
    let body = body_of(
        &adapter(ctx_with_context_length(4_096), None),
        &long_conversation(30_000),
    );

    let first = body["messages"][0]["content"].as_str().unwrap();
    assert!(
        first.starts_with("[Raw tool output truncated"),
        "the oldest tool result should have been elided, got {first:.40}"
    );
    // The protected tail is untouched.
    assert_eq!(body["messages"].as_array().unwrap().len(), 10);
    assert_eq!(body["messages"][9]["content"], "ok");
}

/// The same conversation on a large-context model is left completely alone —
/// the budget follows the model, with no shared floor between them.
#[test]
fn the_same_conversation_is_untouched_on_a_large_context_model() {
    let body = body_of(
        &adapter(ctx_with_context_length(262_144), None),
        &long_conversation(30_000),
    );

    assert_eq!(
        body["messages"][0]["content"].as_str().unwrap().len(),
        30_000
    );
}

/// An unresolvable model has no budget, and a missing budget must mean "do not
/// truncate" rather than "truncate at zero".
#[test]
fn a_passthrough_context_truncates_nothing() {
    let body = body_of(
        &adapter(ModelContext::passthrough(), None),
        &long_conversation(30_000),
    );

    assert_eq!(
        body["messages"][0]["content"].as_str().unwrap().len(),
        30_000
    );
}

/// A conversation that cannot be trimmed to fit fails here rather than being
/// sent upstream to fail there.
#[test]
fn an_untrimmable_conversation_is_rejected() {
    let adapter = adapter(ctx_with_context_length(1_024), None);
    // A single system prompt: protected, and the only message there is.
    let messages = [AgentMessage::System {
        content: "x".repeat(100_000),
    }];

    let err = adapter.shaped_body(&messages, &[], None).unwrap_err();
    assert!(
        err.to_string().contains("context budget"),
        "unexpected error: {err}"
    );
}

// ── Transport fields ──────────────────────────────────────────────────────

/// The pipeline touches `messages` and sampling keys; everything the adapter
/// needs for transport must come through untouched.
#[test]
fn transport_fields_survive_the_pipeline() {
    let body = body_of(&adapter(strict_turns(), None), &[user("hi")]);
    assert_eq!(body["model"], "m");
    assert_eq!(body["stream"], true);
    assert_eq!(body["return_progress"], true);
}
