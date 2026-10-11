//! The messages after a drawn picture, through the loop `gglib chat`
//! composes for a model that reads images and a llama-server that keeps
//! each request: the picture is sent on the message after it, on that
//! request and the next, and is in no saved row and not in the context the
//! next message is sent with.

use std::sync::Arc;

use gglib_core::domain::agent::{AssistantContent, ToolCall};
use gglib_core::request_pipeline::{
    DRAW_TOOL_NAME, DRAWN_PICTURE_LINE, ModelContext, SamplingLayers,
};
use gglib_runtime::{LoopGeneration, compose_agent_loop_with_sampling};
use serde_json::{Value, json};

use super::turn_draw_tests::{DrawsNothing, WORDS, answering};
use super::*;
use crate::bootstrap::test_context;
use crate::target::TurnModel;

/// A PNG's signature and `IHDR`, one pixel square.
fn png() -> Vec<u8> {
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    bytes.extend([0, 0, 0, 13]);
    bytes.extend(b"IHDR");
    bytes.extend([0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0, 0, 0, 0, 0]);
    bytes
}

/// A chat in which `picture` was drawn, and the message sent after it.
fn after_a_draw(picture: &gglib_core::domain::AttachmentId) -> Vec<AgentMessage> {
    let call = ToolCall {
        id: "c1".to_owned(),
        name: DRAW_TOOL_NAME.to_owned(),
        arguments: json!({ "prompt": "a fox" }),
    };
    vec![
        AgentMessage::user("draw a fox"),
        AgentMessage::Assistant {
            content: AssistantContent {
                text: None,
                tool_calls: vec![call],
            },
        },
        AgentMessage::Tool {
            tool_call_id: "c1".to_owned(),
            content: "Drew 1 image, 1x1 PNG, with flux in 9 s; the user can see it, you cannot."
                .to_owned(),
            images: vec![picture.clone()],
        },
        AgentMessage::Assistant {
            content: AssistantContent {
                text: Some("I asked for a red fox.".to_owned()),
                tool_calls: Vec::new(),
            },
        },
        AgentMessage::user("what is in the corner?"),
    ]
}

#[tokio::test]
async fn the_messages_after_a_drawn_picture_carry_it_and_no_row_or_later_context_does() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    let (port, bodies) = answering(&[WORDS, WORDS]);
    let draw = DrawSwitch::through(Arc::new(DrawsNothing));
    let sees = ModelContext {
        image_input: true,
        ..ModelContext::passthrough()
    };
    let agent = compose_agent_loop_with_sampling(
        format!("http://127.0.0.1:{port}"),
        reqwest::Client::new(),
        Some("qwen".to_owned()),
        sees,
        Arc::clone(&ctx.mcp),
        None,
        None,
        None,
        SamplingLayers::default(),
        None,
        None,
        None,
        Some(gglib_core::retry::RetryPolicy::default()),
        None,
        ctx.app.attachments().store(),
        LoopGeneration {
            gate: None,
            drawing: draw.tool(&ctx),
        },
    );
    let made_by = TurnModel::here("qwen".to_owned(), None).made_by();
    let chat = Conversation::create(ctx.app.chat_history(), None, None, made_by).await;
    let chat = chat.expect("a conversation");
    let picture = ctx.app.attachments().ingest(&png()).await.expect("stored");
    let picture = picture.info.id;

    let asked = after_a_draw(&picture);
    let config = AgentConfig::default();
    let mut after = send(
        &agent,
        asked,
        config.clone(),
        false,
        Some(&chat),
        true,
        &draw,
    )
    .await;

    let context = serde_json::to_string(&after).expect("the next message's context");
    assert_eq!(after.len(), 6, "the five sent and the answer: {context}");
    assert!(!context.contains(DRAWN_PICTURE_LINE), "{context}");
    assert!(
        matches!(&after[4], AgentMessage::User { content, images }
            if content == "what is in the corner?" && images.is_empty()),
        "{context}"
    );

    after.push(AgentMessage::user("thanks"));
    send(&agent, after, config, false, Some(&chat), true, &draw).await;

    let bodies = bodies.lock().unwrap().clone();
    assert_eq!(bodies.len(), 2, "one request a message");
    let carrying = |body: &Value| {
        let sent = body["messages"].as_array().expect("messages");
        let at = sent.iter().position(|m| m["content"].is_array());
        let at = at.expect("a message of parts");
        assert_eq!(sent[at]["role"], "user");
        (at, serde_json::to_string(&sent[..=at]).expect("a prefix"))
    };
    let (at, first) = carrying(&bodies[0]);
    let parts = bodies[0]["messages"][at]["content"].as_array().unwrap();
    assert_eq!(
        parts[0],
        json!({ "type": "text", "text": DRAWN_PICTURE_LINE })
    );
    assert_eq!(parts[1]["type"], "image_url");
    assert_eq!(
        parts[2],
        json!({ "type": "text", "text": "what is in the corner?" })
    );
    assert_eq!(carrying(&bodies[1]), (at, first), "the prefix moved");
    let later = bodies[1]["messages"].as_array().unwrap();
    assert_eq!(later[later.len() - 1]["content"], "thanks");
    assert_eq!(bodies[1].to_string().matches("\"image_url\":").count(), 1);

    let rows = ctx.app.chat_history().get_messages(chat.id).await;
    let rows = rows.expect("read");
    assert_eq!(rows.len(), 4, "two messages and two answers: {rows:?}");
    for row in &rows {
        assert!(!row.content.contains(DRAWN_PICTURE_LINE), "{row:?}");
        assert!(row.images.is_empty(), "{row:?}");
    }
}
