//! A daemon turn runs on the context of the model its port serves, so each
//! stage of the request pipeline that reads that context applies to the
//! page's run and to a paired device's turn: the class floor, capability
//! shaping, the truncation budget, the effort gate and the response parser.

use gglib_core::domain::agent::{AgentEvent, AgentMessage};
use gglib_core::domain::{InferenceConfig, ModelCapabilities, TemplateCaps};
use gglib_core::normalize::tags::FORMAT_QWEN_XML;
use serde_json::json;

use crate::handlers::agent::run_fixture::state;
use crate::handlers::agent::turn_fixture::{
    TOOL, Turn, doors, model, page, sampling_of, sent, turn,
};

/// A reasoning-tagged model with no recipe of its own is sent the reasoning
/// floor: the two penalties its class asserts, which an untagged model's
/// floor leaves to llama-server.
#[tokio::test]
async fn a_reasoning_model_with_no_recipe_is_sent_the_reasoning_floor() {
    let (_dir, state) = state().await;
    let id = model(&state, |entry| {
        entry.tags = vec!["reasoning".to_owned()];
    })
    .await;

    for (door, chat) in doors(&state, id, false).await {
        let body = sent(&state, id, chat).await;
        assert_eq!(
            sampling_of(&body),
            InferenceConfig::reasoning_floor(),
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
    let id = model(&state, |entry| {
        entry.capabilities = ModelCapabilities::SUPPORTS_SYSTEM_ROLE;
    })
    .await;
    // Every tool, which is what the page asks for with no filter.
    let every_tool = page(&state, json!({})).await;
    let [_, (_, named)] = doors(&state, id, true).await;

    for chat in [every_tool, named] {
        let body = sent(&state, id, chat).await;
        assert_eq!(body.get("tools"), None);
        assert_eq!(body.get("tool_choice"), None);
        assert_eq!(body["temperature"], json!(0.7_f32));
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

/// A conversation larger than the model's context has its oldest tool
/// results elided before it is sent; one the model has room for is sent
/// whole.
#[tokio::test]
async fn a_conversation_over_the_models_context_is_trimmed_before_it_is_sent() {
    let conversation = || {
        // ~60k characters of tool output, outside the protected tail.
        let mut messages: Vec<_> = ["call_1", "call_2"]
            .map(|id| AgentMessage::Tool {
                tool_call_id: id.to_owned(),
                content: "x".repeat(30_000),
            })
            .into();
        messages.extend((0..8).map(|_| AgentMessage::user("ok")));
        messages
    };
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
