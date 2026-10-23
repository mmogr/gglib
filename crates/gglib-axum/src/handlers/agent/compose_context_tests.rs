//! A daemon turn runs on the context of the model its port serves, so each
//! stage of the request pipeline that reads that context applies to the
//! page's run and to a paired device's turn: the `reasoning` recipe,
//! capability shaping, the truncation budget, the effort gate and the
//! response parser.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use gglib_core::domain::agent::{AgentEvent, AgentMessage, AssistantContent};
use gglib_core::domain::{InferenceConfig, ModelCapabilities, TemplateCaps};
use gglib_core::normalize::tags::FORMAT_QWEN_XML;
use gglib_core::ports::{
    Admission, AdmissionLease, AdmissionRelease, LaunchOverrides, ModelRuntimeError,
    ModelRuntimePort, RunningTarget,
};
use serde_json::json;

use crate::handlers::agent::run_fixture::state;
use crate::handlers::agent::turn_fixture::{
    TOOL, Turn, doors, model, page, sampling_of, sent, sent_held, turn,
};

/// A reasoning-tagged model with nothing stored is sent the `reasoning`
/// recipe, which the tag supplies; an untagged model is sent no penalty, and
/// llama-server's own defaults apply.
#[tokio::test]
async fn a_reasoning_model_with_nothing_stored_is_sent_the_reasoning_recipe() {
    let (_dir, state) = state().await;
    let id = model(&state, |entry| {
        entry.tags = vec!["reasoning".to_owned()];
    })
    .await;

    for (door, chat) in doors(&state, id, false).await {
        let body = sent(&state, id, chat).await;
        assert_eq!(
            sampling_of(&body),
            InferenceConfig::reasoning_profile(),
            "{door}"
        );
    }

    let (_dir, plain) = crate::handlers::agent::run_fixture::state().await;
    let untagged = model(&plain, |_| {}).await;
    let chat = page(&plain, json!({ "tool_filter": [] })).await;
    let body = sent(&plain, untagged, chat).await;
    assert_eq!(body.get("presence_penalty"), None);
    assert_eq!(body.get("min_p"), None);
}

/// A model the catalogue says cannot call tools is sent none, whatever the
/// turn would expose, and so is not capped as a turn with tools is.
#[tokio::test]
async fn a_model_that_cannot_call_tools_is_sent_none_and_is_not_capped() {
    let (_dir, state) = state().await;
    // A recipe nobody here reviewed, which a turn with tools would cap.
    let id = model(&state, |entry| {
        entry.capabilities = ModelCapabilities::SUPPORTS_SYSTEM_ROLE;
        entry.inference_defaults = Some(InferenceConfig {
            temperature: Some(0.9),
            ..InferenceConfig::default()
        });
        entry.defaults_origin = Some(gglib_core::domain::DefaultsOrigin::Published);
    })
    .await;
    // Every tool, which is what the page asks for with no filter.
    let every_tool = page(&state, json!({})).await;
    let [_, (_, named)] = doors(&state, id, true).await;

    for chat in [every_tool, named] {
        let body = sent(&state, id, chat).await;
        assert_eq!(body.get("tools"), None);
        assert_eq!(body.get("tool_choice"), None);
        assert_eq!(body["temperature"], json!(0.9_f32));
    }
}

/// A model whose template enforces alternating turns is sent consecutive
/// messages of one role merged.
#[tokio::test]
async fn a_strict_turn_model_is_sent_consecutive_messages_merged() {
    let (_dir, state) = state().await;
    let id = model(&state, |entry| {
        entry.capabilities |= ModelCapabilities::REQUIRES_STRICT_TURNS;
    })
    .await;

    for (door, mut chat) in doors(&state, id, false).await {
        chat.messages = vec![AgentMessage::user("one"), AgentMessage::user("two")];
        let body = sent(&state, id, chat).await;
        let messages = body["messages"].as_array().expect("messages");
        assert_eq!(messages.len(), 1, "{door}");
        assert_eq!(messages[0]["content"], "one\n\ntwo", "{door}");
    }
}

/// ~60k characters of tool output, outside the protected tail.
fn conversation() -> Vec<AgentMessage> {
    let mut messages: Vec<_> = ["call_1", "call_2"]
        .map(|id| AgentMessage::Tool {
            tool_call_id: id.to_owned(),
            content: "x".repeat(30_000),
            images: Vec::new(),
        })
        .into();
    messages.extend((0..8).map(|_| AgentMessage::user("ok")));
    messages
}

/// A conversation larger than the model's context has its oldest tool
/// results elided before it is sent; one the model has room for is sent
/// whole.
#[tokio::test]
async fn a_conversation_over_the_models_context_is_trimmed_before_it_is_sent() {
    for (context, trimmed) in [(4_096, true), (262_144, false)] {
        let (_dir, state) = state().await;
        // 4096 tokens is a budget of about 16k characters.
        let id = model(&state, |entry| entry.context_length = Some(context)).await;

        for (door, mut chat) in doors(&state, id, false).await {
            chat.messages = conversation();
            let body = sent(&state, id, chat).await;
            let oldest = body["messages"][0]["content"].as_str().expect("text");
            assert_eq!(
                oldest.starts_with("[Raw tool output truncated"),
                trimmed,
                "{door}"
            );
            assert_eq!(oldest.len() == 30_000, !trimmed, "{door}");
            assert_eq!(body["messages"].as_array().map(Vec::len), Some(10));
        }
    }
}

/// A runtime that holds whatever model a run asks for and reports it, in its
/// primary slot, launched at the context its first field names.
#[derive(Debug)]
pub(crate) struct LaunchedAt(pub(crate) u64, pub(crate) Mutex<Option<(u16, u32)>>);

#[derive(Debug)]
struct Unreleased;

impl AdmissionRelease for Unreleased {
    fn release(&self, _slot: usize) {}

    fn progress(&self, _slot: usize) {}
}

#[async_trait]
impl ModelRuntimePort for LaunchedAt {
    async fn admit(
        &self,
        _model_name: &str,
        _num_ctx: Option<u64>,
        _default_ctx: Option<u64>,
        _overrides: LaunchOverrides,
    ) -> Result<Admission, ModelRuntimeError> {
        Err(ModelRuntimeError::Internal("not asked here".to_owned()))
    }

    async fn current_model(&self) -> Option<RunningTarget> {
        let (port, id) = (*self.1.lock().unwrap())?;
        Some(RunningTarget::local(
            port,
            id,
            "served".to_owned(),
            self.0,
            false,
        ))
    }

    async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
        Ok(())
    }

    async fn stop_model(&self, _model_id: u32) -> Result<bool, ModelRuntimeError> {
        Ok(false)
    }

    fn hold(&self, port: u16, model_id: u32) -> Option<AdmissionLease> {
        *self.1.lock().unwrap() = Some((port, model_id));
        Some(AdmissionLease::new(Arc::new(Unreleased), 0))
    }
}

/// A run that holds its model is trimmed to the context that model was
/// launched with, not the larger one it was trained for (#1272). The same
/// turn prepared without the hold keeps the trained budget and is sent
/// whole: the control.
#[tokio::test]
async fn a_held_run_is_trimmed_to_its_models_launched_context() {
    let (_dir, mut state) = state().await;
    Arc::get_mut(&mut state).expect("not yet shared").runtime =
        Arc::new(LaunchedAt(4_096, Mutex::new(None)));
    let id = model(&state, |entry| entry.context_length = Some(262_144)).await;
    let oldest = |body: &serde_json::Value| body["messages"][0]["content"].as_str().map(str::len);

    for (door, mut chat) in doors(&state, id, false).await {
        chat.messages = conversation();
        let body = sent_held(&state, id, chat).await;
        let first = body["messages"][0]["content"].as_str().expect("text");
        assert!(first.starts_with("[Raw tool output truncated"), "{door}");
    }
    for (door, mut chat) in doors(&state, id, false).await {
        chat.messages = conversation();
        let body = sent(&state, id, chat).await;
        assert_eq!(oldest(&body), Some(30_000), "{door}");
    }
}

/// A long earlier reply in a chat with no tools is trimmed to a line that
/// names it the assistant's own, not raw tool output (#1271).
#[tokio::test]
async fn a_trimmed_reply_is_named_as_the_assistants_own() {
    let (_dir, state) = state().await;
    let id = model(&state, |entry| entry.context_length = Some(4_096)).await;

    for (door, mut chat) in doors(&state, id, false).await {
        let mut messages = vec![AgentMessage::Assistant {
            content: AssistantContent {
                text: Some("x".repeat(60_000)),
                tool_calls: Vec::new(),
            },
        }];
        messages.extend((0..8).map(|_| AgentMessage::user("ok")));
        chat.messages = messages;
        let body = sent(&state, id, chat).await;
        let oldest = body["messages"][0]["content"].as_str().expect("text");
        assert_eq!(
            oldest, "[An earlier assistant reply was shortened to fit the context window.]",
            "{door}"
        );
    }
}

/// An effort level the page names is deleted for a model whose template
/// llama-server reported does not read it, and sent for one never observed.
/// The thinking budget beside it is sent either way.
#[tokio::test]
async fn an_effort_level_the_models_template_does_not_read_is_not_sent() {
    let (_dir, state) = state().await;
    let id = model(&state, |_| {}).await;
    let named = json!({
        "tool_filter": [],
        "reasoning_effort": "high",
        "reasoning_budget_tokens": 512,
    });

    let body = sent(&state, id, page(&state, named.clone()).await).await;
    assert_eq!(body["reasoning_effort"], "high", "never observed");
    assert_eq!(body["reasoning_budget_tokens"], 512);

    let unread = TemplateCaps {
        supports_reasoning_effort: Some(false),
        ..TemplateCaps::default()
    };
    let catalog_id = u32::try_from(id).unwrap();
    state
        .catalog
        .record_template_caps(catalog_id, unread)
        .await
        .unwrap();
    let body = sent(&state, id, page(&state, named).await).await;
    assert_eq!(body.get("reasoning_effort"), None);
    assert_eq!(body["reasoning_budget_tokens"], 512);
}

/// A reply in a dialect model's tool-call markup is read as the call it is:
/// the loop dispatches it. From a model with no dialect the same reply is
/// the text it looks like.
#[tokio::test]
async fn a_dialect_models_tool_call_markup_is_read_as_a_call() {
    let markup = format!(
        "<tool_call>\n{}\n</tool_call>",
        json!({ "name": TOOL, "arguments": {} })
    );
    let reply = format!(
        "data: {}\n\ndata: {}\n\ndata: [DONE]\n\n",
        json!({ "choices": [{ "delta": { "content": markup } }] }),
        json!({ "choices": [{ "delta": {}, "finish_reason": "stop" }] }),
    );
    let called = |turn: &Turn| {
        turn.events.iter().find_map(|event| match event {
            AgentEvent::ToolCallStart { tool_call, .. } => Some(tool_call.name.clone()),
            _ => None,
        })
    };
    // One iteration: the model server gives every request the same reply.
    let once = json!({ "config": { "max_iterations": 1 } });

    let (_dir, tagged) = state().await;
    let dialect = model(&tagged, |entry| {
        entry.tags = vec![FORMAT_QWEN_XML.to_owned()];
    })
    .await;
    let chat = page(&tagged, once.clone()).await;
    let parsed = turn(&tagged, dialect, chat, &reply).await;
    assert_eq!(
        called(&parsed).as_deref(),
        Some(TOOL),
        "{:?}",
        parsed.events
    );

    let (_dir, untagged) = state().await;
    let plain = model(&untagged, |_| {}).await;
    let chat = page(&untagged, once).await;
    let unparsed = turn(&untagged, plain, chat, &reply).await;
    assert_eq!(called(&unparsed), None, "{:?}", unparsed.events);
    assert!(unparsed.ended_well, "the markup was the answer");
}
