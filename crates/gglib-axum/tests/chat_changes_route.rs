//! A chat read with its branch points, and a change made to it over HTTP
//! (ADR 0017): an edit that would rewrite a reply branches, a change while a
//! reply is being written branches rather than waiting, and a refused change
//! says why and writes nothing.

mod common;

use axum::Router;
use axum::http::{Method, StatusCode};
use gglib_app_services::{Reservation, RunSpec};
use gglib_core::CorsConfig;
use gglib_core::domain::chat::{MessageRole, NewConversation, NewMessage};
use gglib_core::domain::runs::RunKind;
use gglib_core::ports::RunScope;
use serde_json::{Value, json};

use common::harness::test_state_and_app;
use common::origin::call_json;

/// A chat holding `rows`; its id and theirs.
async fn chat(state: &gglib_axum::AppState, rows: &[(MessageRole, &str)]) -> (i64, Vec<i64>) {
    let history = state.core.chat_history();
    let id = history
        .create_conversation(NewConversation {
            title: "Kyoto".into(),
            ..NewConversation::default()
        })
        .await
        .unwrap();
    let mut ids = Vec::new();
    for (role, content) in rows {
        let row = NewMessage {
            conversation_id: id,
            role: *role,
            content: (*content).to_owned(),
            metadata: None,
            images: Vec::new(),
        };
        ids.push(history.save_message(row).await.unwrap());
    }
    (id, ids)
}

const KYOTO: [(MessageRole, &str); 4] = [
    (MessageRole::User, "Plan a trip to Kyoto"),
    (MessageRole::Assistant, "Day 1: temples"),
    (MessageRole::User, "Make it cheaper"),
    (MessageRole::Assistant, "Hostels and buses."),
];

async fn thread(app: &Router, id: i64) -> (StatusCode, Value) {
    let uri = format!("/api/conversations/{id}/thread");
    call_json(app, Method::GET, &uri, None).await
}

async fn change(app: &Router, id: i64, body: Value) -> (StatusCode, Value) {
    let uri = format!("/api/conversations/{id}/changes");
    call_json(app, Method::POST, &uri, Some(body)).await
}

#[tokio::test]
async fn an_edit_of_an_answered_question_opens_a_branch_both_chats_offer() {
    let (state, app) = test_state_and_app(CorsConfig::AllowAll).await;
    let (id, ids) = chat(&state, &KYOTO).await;

    let edit = json!({"kind": "edit", "message_id": ids[2], "content": "Make it shorter"});
    let (status, changed) = change(&app, id, edit).await;

    assert_eq!(status, StatusCode::OK, "{changed}");
    assert_eq!(
        (changed["forked"].clone(), changed["answer"].clone()),
        (json!(true), json!(true))
    );
    let branch = changed["conversation_id"].as_i64().unwrap();
    let (status, read) = thread(&app, branch).await;
    assert_eq!(status, StatusCode::OK, "{read}");
    assert_eq!(read["messages"].as_array().unwrap().len(), 3);
    assert_eq!(read["answerable"], json!(true));
    assert_eq!(read["points"][0]["index"], json!(1));
    let (_, source) = thread(&app, id).await;
    assert_eq!(source["messages"].as_array().unwrap().len(), 4);
    assert_eq!(
        source["points"][0]["options"][1]["conversation_id"],
        json!(branch)
    );
    assert!(source.get("answerable").is_none(), "{source}");
}

/// While a reply is being written, the question it answers is not replaced
/// under it: the edit branches instead of waiting.
#[tokio::test]
async fn an_edit_of_a_question_being_answered_branches() {
    let (state, app) = test_state_and_app(CorsConfig::AllowAll).await;
    let (id, ids) = chat(&state, &KYOTO[..3]).await;
    let spec = RunSpec {
        kind: RunKind::Agent,
        model: None,
        conversation_id: Some(id),
    };
    let Ok(Reservation::New(_writing)) = state.runs.reserve(RunScope::Local, "e1", spec) else {
        panic!("reserved");
    };

    let edit = json!({"kind": "edit", "message_id": ids[2], "content": "Make it shorter"});
    let (status, changed) = change(&app, id, edit).await;

    assert_eq!(status, StatusCode::OK, "{changed}");
    assert_eq!(changed["forked"], json!(true));
    let rows = state.core.chat_history().get_messages(id).await.unwrap();
    assert_eq!(rows[2].content, "Make it cheaper");
}

#[tokio::test]
async fn a_refused_change_says_why_and_writes_nothing() {
    let (state, app) = test_state_and_app(CorsConfig::AllowAll).await;
    let (id, ids) = chat(&state, &KYOTO).await;

    for (body, status, code) in [
        (
            json!({"kind": "regenerate", "message_id": ids[0]}),
            StatusCode::BAD_REQUEST,
            "not_a_reply",
        ),
        (
            json!({"kind": "branch", "message_id": 999}),
            StatusCode::NOT_FOUND,
            "message_not_found",
        ),
        (
            json!({"kind": "edit", "message_id": ids[2], "content": "Make it cheaper"}),
            StatusCode::BAD_REQUEST,
            "unchanged",
        ),
    ] {
        let (answered, error) = change(&app, id, body).await;
        assert_eq!(answered, status, "{error}");
        assert_eq!(error["type"], code);
    }
    let (answered, error) = change(
        &app,
        id + 1,
        json!({"kind": "branch", "message_id": ids[0]}),
    )
    .await;
    assert_eq!(answered, StatusCode::NOT_FOUND, "{error}");
    assert_eq!(error["type"], "conversation_not_found");
    assert_eq!(
        state
            .core
            .chat_history()
            .list_conversations()
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn a_change_with_a_key_it_does_not_take_is_refused() {
    let (state, app) = test_state_and_app(CorsConfig::AllowAll).await;
    let (id, ids) = chat(&state, &KYOTO).await;
    let (status, _) = change(
        &app,
        id,
        json!({"kind": "branch", "message_id": ids[0], "replace_from": 1}),
    )
    .await;
    assert!(status.is_client_error(), "{status}");
}

/// A picture kept beside the one it was made from is on that picture's tool
/// row when the chat is read again, last; what cannot be kept is refused
/// with a code and changes nothing.
#[tokio::test]
async fn a_kept_picture_is_read_back_last_on_its_tool_row() {
    let (state, app) = test_state_and_app(CorsConfig::LocalOnly).await;
    let store = state.core.attachments();
    let drawn = store.ingest(&png_bytes(64, 64)).await.unwrap().info;
    let fixed = store.ingest(&png_bytes(64, 32)).await.unwrap().info;
    let history = state.core.chat_history();
    let id = history
        .create_conversation(NewConversation {
            title: "A fox".into(),
            ..NewConversation::default()
        })
        .await
        .unwrap();
    let row = NewMessage {
        conversation_id: id,
        role: MessageRole::Tool,
        content: "Drew 1 image.".to_owned(),
        metadata: None,
        images: vec![drawn.id.clone()],
    };
    let row = history.save_message(row).await.unwrap();
    let app = &app;
    let keep = |from: &str, image: &str| {
        let body = json!({"from": from, "image": image});
        async move {
            let uri = format!("/api/conversations/{id}/pictures");
            call_json(app, Method::POST, &uri, Some(body)).await
        }
    };

    let (status, kept) = keep(drawn.id.as_str(), fixed.id.as_str()).await;
    assert_eq!(status, StatusCode::OK, "{kept}");
    let ids: Vec<&str> = kept
        .as_array()
        .unwrap()
        .iter()
        .map(|image| image["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, [drawn.id.as_str(), fixed.id.as_str()]);
    assert_eq!(kept[1]["height"], 32);

    let (_, read) = thread(app, id).await;
    let saved = read["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|message| message["id"] == row)
        .unwrap();
    assert_eq!(saved["images"][1]["id"], fixed.id.as_str());

    let unknown = "0".repeat(64);
    // An id a body names that is not there is a 400, as on every route
    // that takes one.
    for (from, image) in [
        (unknown.as_str(), fixed.id.as_str()),
        (drawn.id.as_str(), unknown.as_str()),
    ] {
        let (status, refused) = keep(from, image).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{refused}");
        assert_eq!(refused["type"], "attachment_not_found", "{refused}");
    }
    let (_, read) = thread(app, id).await;
    assert_eq!(read["messages"][0]["images"].as_array().unwrap().len(), 2);

    let uri = "/api/conversations/999/pictures";
    let body = json!({"from": drawn.id.as_str(), "image": fixed.id.as_str()});
    let (status, refused) = call_json(app, Method::POST, uri, Some(body)).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{refused}");
    assert_eq!(refused["type"], "conversation_not_found", "{refused}");
}

/// A PNG's signature and `IHDR`, `width` by `height`: what the store reads.
fn png_bytes(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13];
    bytes.extend(b"IHDR");
    bytes.extend(width.to_be_bytes());
    bytes.extend(height.to_be_bytes());
    bytes.extend([8, 6, 0, 0, 0]);
    bytes
}
