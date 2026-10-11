//! The message after a drawn picture, by the two doors that are sent their
//! history: the page's run, whose tool message names the picture by id,
//! and a chat run with builtins, whose client keeps the chat and the
//! picture and sends it inline on its own message. The first is shown the
//! picture by the daemon; the second is sent on as it came, with nothing
//! added a second time.

use gglib_core::request_pipeline::{DRAW_TOOL_NAME, DRAWN_NOTE, DRAWN_PICTURE_LINE};
use serde_json::{Value, json};

use crate::handlers::agent::chat_turn::read;
use crate::handlers::agent::image_gate::image_gate_tests::{model, uploaded};
use crate::handlers::agent::run_fixture::{End, conversation, prepared, saved, settled, start};
use crate::handlers::agent::run_fixture::{drawing_state, state};
use crate::handlers::agent::turn_fixture::{self, REPLY, page, turn};
use crate::state::AppState;

const DREW: &str = "Drew 1 image, 1x1 PNG, with flux in 9 s; the user can see it, you cannot.";

/// The server the loop is run against, its model linked to a projector or
/// not. Its id.
async fn server(state: &AppState, sees: bool) -> i64 {
    let served = turn_fixture::model(state, |_| {}).await;
    if sees {
        let models = state.core.models();
        let mut seeing = models.get_by_id(served).await.unwrap().unwrap();
        seeing.projector_path = Some("models/mmproj-F16.gguf".into());
        models.update(&seeing).await.unwrap();
    }
    served
}

/// The page's messages for the message after a draw: the tool message
/// names `picture`.
fn page_history(picture: &str) -> Value {
    json!([
        { "role": "user", "content": "draw a fox" },
        { "role": "assistant", "tool_calls": [
            { "id": "c1", "name": DRAW_TOOL_NAME, "arguments": { "prompt": "a fox" } },
        ]},
        { "role": "tool", "tool_call_id": "c1", "content": DREW, "images": [picture] },
        { "role": "assistant", "content": "I asked for a red fox." },
        { "role": "user", "content": "what is in the corner?" },
    ])
}

/// A page's run for a model that reads images: the picture is on the
/// message after it, once, and the tool message is its text. Saved as a
/// run's rows are, the turn has no row that holds the line or an image,
/// and no event of its loop does.
#[tokio::test]
async fn a_page_run_after_a_draw_shows_the_picture_and_saves_none_of_it() {
    let (_dir, state) = drawing_state().await;
    let picture = uploaded(&state).await;
    let id = server(&state, true).await;
    let body = json!({ "tool_filter": [], "messages": page_history(picture.as_str()) });

    let turn = turn(&state, id, page(&state, body).await, REPLY).await;

    assert!(turn.ended_well, "{:?}", turn.events);
    assert_eq!(turn.requests.len(), 1);
    let request = &turn.requests[0];
    let sent = request["messages"].as_array().unwrap();
    assert_eq!(sent.len(), 6);
    assert_eq!(sent[0], json!({ "role": "system", "content": DRAWN_NOTE }));
    assert_eq!(
        sent[3],
        json!({ "role": "tool", "tool_call_id": "c1", "content": DREW })
    );
    let parts = sent[5]["content"].as_array().expect("parts");
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
    let events = format!("{:?}", turn.events);
    assert!(!events.contains(DRAWN_PICTURE_LINE), "{events}");
    assert!(!events.contains("base64"), "{events}");

    let chat = conversation(&state).await;
    let (run, _) = prepared(turn.events, End::Finish);
    start(&state, "r1", Some(chat), run).await;
    settled(&state).await;
    let rows = saved(&state, chat).await;
    assert!(!rows.is_empty());
    for row in &rows {
        assert!(!row.content.contains(DRAWN_PICTURE_LINE), "{row:?}");
        assert!(row.images.is_empty(), "{row:?}");
    }
}

/// The same run for a model with no projector is not refused, and is sent
/// the chat with no image and no line.
#[tokio::test]
async fn a_page_run_after_a_draw_is_not_refused_for_a_model_that_reads_no_images() {
    let (_dir, state) = drawing_state().await;
    let picture = uploaded(&state).await;
    let id = server(&state, false).await;
    let body = json!({ "tool_filter": [], "messages": page_history(picture.as_str()) });

    let turn = turn(&state, id, page(&state, body).await, REPLY).await;

    assert!(turn.ended_well, "{:?}", turn.events);
    let request = &turn.requests[0];
    assert_eq!(request["messages"][0]["content"], DRAWN_NOTE);
    assert_eq!(request["messages"][5]["content"], "what is in the corner?");
    assert!(!request.to_string().contains("image_url"), "{request}");
    assert!(!request.to_string().contains(DRAWN_PICTURE_LINE));
}

/// A client that keeps its own chats holds the picture, and shows it to
/// the model by sending it on its own message as it sends any image, after
/// the line. The daemon reads that as the person's image and adds nothing:
/// one line, one image.
#[tokio::test]
async fn a_chat_run_with_builtins_sends_on_the_picture_its_client_sent_and_adds_none() {
    let (_dir, state) = state().await;
    model(&state, "qwen-vision", true).await;
    let served = server(&state, true).await;
    let picture = uploaded(&state).await;
    let blob = state.core.attachments().blob(&picture).await.unwrap();
    let inline = blob.data_url();
    let body = json!({ "model": "qwen-vision", "messages": [
        { "role": "user", "content": "draw a fox" },
        { "role": "assistant", "content": null, "tool_calls": [
            { "id": "c1", "type": "function", "function": {
                "name": DRAW_TOOL_NAME, "arguments": "{\"prompt\":\"a fox\"}" } } ] },
        { "role": "tool", "tool_call_id": "c1", "content": DREW },
        { "role": "assistant", "content": "I asked for a red fox." },
        { "role": "user", "content": [
            { "type": "text", "text": DRAWN_PICTURE_LINE },
            { "type": "image_url", "image_url": { "url": inline } },
            { "type": "text", "text": "what is in the corner?" },
        ] },
    ] });

    let (_, chat) = read(&state, &body, false.into()).await.unwrap();
    let turn = turn(&state, served, chat, REPLY).await;

    assert!(turn.ended_well, "{:?}", turn.events);
    let request = turn.requests[0].to_string();
    assert_eq!(request.matches("\"image_url\":").count(), 1, "{request}");
    assert_eq!(request.matches(DRAWN_PICTURE_LINE).count(), 1, "{request}");
    assert_eq!(request.matches(DRAWN_NOTE).count(), 1, "{request}");
    let said = format!("{DRAWN_PICTURE_LINE}\nwhat is in the corner?");
    assert_eq!(
        turn.requests[0]["messages"][5]["content"],
        json!([
            { "type": "text", "text": said },
            { "type": "image_url", "image_url": { "url": inline } },
        ])
    );
}

/// A run on the paired machine's model is shown the picture exactly when
/// that machine's answer to the lookup says the model reads images: it is
/// all this machine knows of the model, and the adapter asks nothing else.
#[tokio::test]
async fn a_far_run_shows_the_picture_when_the_far_machine_says_the_model_reads_images() {
    use gglib_core::domain::{Machine, ModelRef};
    use gglib_core::request_pipeline::ModelContext;

    use crate::handlers::agent::remote_upstream::remote;
    use crate::handlers::remote::fake_far::{FINGERPRINT, far};

    let lookup = |sees: bool| {
        json!({ "detail": {
            "id": 3, "name": "org/qwen3", "paramCountB": 8.0, "quantization": "Q4_K_M",
            "addedAt": "2026-10-01 09:00:00", "isServing": false, "metadata": {},
            "imageInput": sees,
        } })
        .to_string()
    };
    let named = ModelRef {
        machine: Machine::Paired {
            fingerprint: FINGERPRINT.to_owned(),
        },
        id: 3,
    };

    for sees in [true, false] {
        let (_fake, proxy) = far(200, &lookup(sees)).await;
        let upstream = remote(&proxy, &named).await.expect("looked up");
        let want = ModelContext {
            image_input: sees,
            ..ModelContext::passthrough()
        };
        assert_eq!(upstream.model_context, want);
    }
}
