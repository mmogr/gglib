//! A device's turn on a chat that holds a drawn picture: the hub builds the
//! history from its own rows, the tool row's picture among them. A model
//! that reads images is sent the picture on the message after it; a model
//! that does not is sent none, and is not refused for it. Either way no row
//! is changed.

use gglib_core::domain::AttachmentId;
use gglib_core::domain::agent::AgentMessage;
use gglib_core::domain::chat::{MessageRole, NewMessage};
use gglib_core::request_pipeline::{DRAW_TOOL_NAME, DRAWN_NOTE, DRAWN_PICTURE_LINE};
use serde_json::{Value, json};

use super::hub_turn_tests::{chat, turn};
use super::plan;
use crate::handlers::agent::image_gate::image_gate_tests::{model, uploaded};
use crate::handlers::agent::run_fixture::{saved, state};
use crate::handlers::agent::turn_fixture::{self, REPLY};
use crate::state::AppState;

/// The model the chat fixture's last reply was made by.
const CHAT_MODEL: &str = "qwen3-8b";

const DREW: &str = "Drew 1 image, 1x1 PNG, with flux in 9 s; the user can see it, you cannot.";

/// Save a run sent with Draw onto chat `id`: the message, the call, the
/// tool's answer with `picture`, and the closing words.
async fn draw_turn(state: &AppState, id: i64, picture: &AttachmentId) {
    let call = json!([{ "id": "c1", "name": DRAW_TOOL_NAME, "arguments": { "prompt": "a fox" } }]);
    let made_by = json!({ "modelName": CHAT_MODEL });
    let rows = [
        (MessageRole::User, "draw a fox", None, Vec::new()),
        (
            MessageRole::Assistant,
            "",
            Some(json!({ "tool_calls": call, "modelName": CHAT_MODEL })),
            Vec::new(),
        ),
        (
            MessageRole::Tool,
            DREW,
            Some(json!({ "tool_call_id": "c1" })),
            vec![picture.clone()],
        ),
        (
            MessageRole::Assistant,
            "I asked for a fox.",
            Some(made_by),
            Vec::new(),
        ),
    ];
    for (role, content, metadata, images) in rows {
        let row = NewMessage {
            conversation_id: id,
            role,
            content: content.to_owned(),
            metadata,
            images,
        };
        state.core.chat_history().save_message(row).await.unwrap();
    }
}

/// The one request the model is sent for a device's next message on a chat
/// that holds a drawn picture, the chat's model reading images or not, and
/// the chat's rows before and after.
async fn next_message(sees: bool) -> (Value, Vec<String>, Vec<String>) {
    let (_dir, state) = state().await;
    let picture = uploaded(&state).await;
    let model_id = model(&state, CHAT_MODEL, sees).await;
    let id = chat(&state, None).await;
    draw_turn(&state, id, &picture).await;
    let rows = |rows: Vec<gglib_core::domain::chat::Message>| {
        rows.iter().map(|row| format!("{row:?}")).collect()
    };
    let before: Vec<String> = rows(saved(&state, id).await);

    let planned = plan(&state, turn(id, "what is in the corner?")).await;
    let planned = planned.unwrap_or_else(|e| panic!("the turn was refused: {e}"));
    let carried = planned.chat.messages.iter().any(|message| {
        matches!(message, AgentMessage::Tool { images, .. } if images == std::slice::from_ref(&picture))
    });
    assert!(carried, "the tool row's picture reaches the composer by id");
    let done = turn_fixture::turn(&state, model_id, planned.chat, REPLY).await;

    assert!(done.ended_well, "{:?}", done.events);
    assert_eq!(done.requests.len(), 1);
    let events = format!("{:?}", done.events);
    assert!(!events.contains(DRAWN_PICTURE_LINE), "{events}");
    let after = rows(saved(&state, id).await);
    (done.requests[0].clone(), before, after)
}

#[tokio::test]
async fn a_hub_turn_after_a_draw_shows_a_model_that_reads_images_the_picture() {
    let (request, before, after) = next_message(true).await;

    let sent = request["messages"].as_array().unwrap();
    assert_eq!(sent.len(), 8, "{request}");
    assert_eq!(sent[5]["role"], "tool");
    assert_eq!(sent[5]["content"], DREW);
    assert!(sent[5].get("images").is_none(), "{}", sent[5]);
    let parts = sent[7]["content"].as_array().expect("parts");
    assert_eq!(parts.len(), 3);
    assert_eq!(
        parts[0],
        json!({ "type": "text", "text": DRAWN_PICTURE_LINE })
    );
    let url = parts[1]["image_url"]["url"].as_str().unwrap();
    assert!(url.starts_with("data:image/png;base64,"), "{url}");
    assert_eq!(
        parts[2],
        json!({ "type": "text", "text": "what is in the corner?" })
    );
    assert_eq!(request.to_string().matches("\"image_url\":").count(), 1);
    assert!(sent[0]["content"].as_str().unwrap().ends_with(DRAWN_NOTE));

    assert_eq!(before, after, "a row was changed");
    assert!(!after.concat().contains(DRAWN_PICTURE_LINE));
}

#[tokio::test]
async fn a_hub_turn_after_a_draw_is_not_refused_for_a_model_that_reads_none() {
    let (request, before, after) = next_message(false).await;

    let sent = request["messages"].as_array().unwrap();
    assert_eq!(sent.len(), 8, "{request}");
    assert_eq!(
        sent[7],
        json!({ "role": "user", "content": "what is in the corner?" })
    );
    assert!(!request.to_string().contains("image_url"), "{request}");
    assert!(!request.to_string().contains(DRAWN_PICTURE_LINE));
    assert!(sent[0]["content"].as_str().unwrap().ends_with(DRAWN_NOTE));
    assert_eq!(before, after, "a row was changed");
}
