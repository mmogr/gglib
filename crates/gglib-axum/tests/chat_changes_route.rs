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
