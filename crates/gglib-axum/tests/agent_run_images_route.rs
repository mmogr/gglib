//! `PUT /api/runs/{id}?kind=agent`, this machine's turn, with images: an
//! image not stored, or images over 16 MiB together, the chat's saved ones
//! included, is refused by its code before a slot is taken or a model is
//! reached, on this machine's model and the paired machine's alike
//! (#1257).
//!
//! The harness runs no llama-server and no tunnel, so a turn whose images
//! pass is made, `201`, and then fails where its model cannot load or its
//! paired machine is away. That `201` is each test's proof that the check
//! let it through.

mod common;

use std::time::Duration;

use axum::Router;
use axum::http::{Method, StatusCode};
use gglib_core::CorsConfig;
use gglib_core::contracts::http::daemon::{RUNS_PATH, run_path};
use gglib_core::domain::AttachmentId;
use gglib_core::domain::chat::{ConversationSettings, MessageRole, NewConversation, NewMessage};
use gglib_core::domain::runs::{RunInfo, RunList, RunStatus};
use gglib_core::domain::{Machine, ModelRef};
use gglib_core::request_pipeline::{MAX_IMAGE_BYTES, MAX_REQUEST_IMAGE_BYTES};
use serde_json::{Value, json};

use common::harness::test_state_and_app;
use common::origin::call_json;

/// A PNG's signature and `IHDR`, then `fill` up to `len` bytes.
fn png(fill: u8, len: usize) -> Vec<u8> {
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    bytes.extend([0, 0, 0, 13]);
    bytes.extend(b"IHDR");
    bytes.extend(640_u32.to_be_bytes());
    bytes.extend(480_u32.to_be_bytes());
    bytes.extend([8, 6, 0, 0, 0, 0, 0, 0, 0]);
    bytes.resize(len.max(bytes.len()), fill);
    bytes
}

async fn upload(state: &gglib_axum::AppState, bytes: &[u8]) -> String {
    let stored = state.core.attachments().ingest(bytes).await.unwrap();
    stored.info.id.to_string()
}

/// A chat on `model`, named `qwen`, with a question that carries
/// `earlier` and its answer.
async fn chat(state: &gglib_axum::AppState, model: Option<ModelRef>, earlier: &[&str]) -> i64 {
    let history = state.core.chat_history();
    let settings = ConversationSettings {
        model,
        model_name: Some("qwen".to_owned()),
        ..ConversationSettings::default()
    };
    let id = history
        .create_conversation(NewConversation {
            title: "t".to_owned(),
            settings: Some(settings),
            ..NewConversation::default()
        })
        .await
        .unwrap();
    let images = earlier.iter().map(|i| AttachmentId::parse(i).unwrap());
    for (role, content, images) in [
        (MessageRole::User, "this one", images.collect()),
        (MessageRole::Assistant, "seen", Vec::new()),
    ] {
        let row = NewMessage {
            conversation_id: id,
            role,
            content: content.to_owned(),
            metadata: None,
            images,
        };
        history.save_message(row).await.unwrap();
    }
    id
}

/// A chat on this machine's `qwen`, by its id, with `earlier` in its
/// history.
async fn here(state: &gglib_axum::AppState, earlier: &[&str]) -> i64 {
    let model = qwen(state).await;
    chat(state, Some(model), earlier).await
}

/// This machine's `qwen`, by its id, catalogued on first use with a
/// projector, so it reads images: a chat names its model by id, and a name
/// alone is never looked up.
async fn qwen(state: &gglib_axum::AppState) -> ModelRef {
    let found = state.core.models().get("qwen").await.unwrap();
    let id = if let Some(model) = found {
        model.id
    } else {
        let mut model = gglib_core::domain::NewModel::new(
            "qwen".to_owned(),
            std::path::PathBuf::from("/models/qwen.gguf"),
            7.0,
            chrono::Utc::now(),
        );
        model.projector_path = Some(std::path::PathBuf::from("/models/mmproj-qwen.gguf"));
        state.core.models().add(model).await.unwrap().id
    };
    ModelRef {
        machine: Machine::Local,
        id,
    }
}

/// A chat on the paired machine's model 3, with `earlier` in its history.
async fn there(state: &gglib_axum::AppState, earlier: &[&str]) -> i64 {
    let model = ModelRef {
        machine: Machine::Paired {
            fingerprint: "0a1b2c3d4e5f".to_owned(),
        },
        id: 3,
    };
    chat(state, Some(model), earlier).await
}

/// Wait for run `id` to end, so the slot it held is free again.
async fn ended(app: &Router, id: &str) {
    let read = async {
        loop {
            let (_, info) = call_json(app, Method::GET, &run_path(id), None).await;
            let info: RunInfo = serde_json::from_value(info).unwrap();
            if !matches!(info.status, RunStatus::Queued | RunStatus::InProgress) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(10), read)
        .await
        .expect("the run ends");
}

/// This machine's turn on `chat`, with `last`.
fn turn(chat: i64, last: &[&str]) -> Value {
    json!({ "conversation_id": chat, "content": "and this?", "images": last })
}

async fn put(app: &Router, id: &str, body: Value) -> (StatusCode, String) {
    let uri = format!("{}?kind=agent", run_path(id));
    let (status, error) = call_json(app, Method::PUT, &uri, Some(body)).await;
    (
        status,
        error["type"].as_str().unwrap_or_default().to_owned(),
    )
}

async fn no_runs(app: &Router) {
    let (_, list) = call_json(app, Method::GET, RUNS_PATH, None).await;
    let list: RunList = serde_json::from_value(list).unwrap();
    assert!(list.runs.is_empty(), "{list:?}");
}

fn refusal(code: &str) -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, code.to_owned())
}

fn made() -> (StatusCode, String) {
    (StatusCode::CREATED, String::new())
}

/// An id the turn names that is not stored is refused by its code, on
/// either machine's model; the same turn with the image stored is made.
#[tokio::test]
async fn an_image_the_turn_names_that_is_not_stored_is_refused_by_code() {
    let (state, app) = test_state_and_app(CorsConfig::AllowAll).await;
    let kept = upload(&state, &png(1, 64)).await;
    let gone = AttachmentId::of(b"never uploaded").to_string();
    let (local, far) = (here(&state, &[]).await, there(&state, &[]).await);

    for chat in [local, far] {
        let refused = put(&app, "a1", turn(chat, &[&gone])).await;
        assert_eq!(refused, refusal("attachment_not_found"), "{chat}");
    }
    no_runs(&app).await;

    assert_eq!(put(&app, "a1", turn(local, &[&kept])).await, made());
    // The local run loads its model, holding the one slot until it ends.
    ended(&app, "a1").await;
    assert_eq!(put(&app, "a2", turn(far, &[&kept])).await, made());
}

/// Two images at 8 MiB each are the 16 MiB a request may carry; the
/// chat's saved image and the turn's add up, and one image more is refused,
/// with nothing saved.
#[tokio::test]
async fn images_over_16_mib_together_are_refused_by_code() {
    let (state, app) = test_state_and_app(CorsConfig::AllowAll).await;
    assert_eq!(MAX_REQUEST_IMAGE_BYTES, 2 * MAX_IMAGE_BYTES);
    let big = upload(&state, &png(2, MAX_IMAGE_BYTES)).await;
    let small = upload(&state, &png(3, 64)).await;
    let (local, far) = (here(&state, &[&big]).await, there(&state, &[&big]).await);

    for chat in [local, far] {
        let over = put(&app, "a1", turn(chat, &[&big, &small])).await;
        assert_eq!(over, refusal("request_images_too_large"), "{chat}");
        let saved = state.core.chat_history().get_messages(chat).await;
        assert_eq!(saved.unwrap().len(), 2, "nothing was saved");
    }
    no_runs(&app).await;

    assert_eq!(put(&app, "a1", turn(local, &[&big])).await, made());
    ended(&app, "a1").await;
    assert_eq!(put(&app, "a2", turn(far, &[&big])).await, made());
}

/// The check comes before a slot is taken: with every slot held, a turn
/// whose image is not stored is still told why.
#[tokio::test]
async fn the_image_refusal_comes_before_the_slot() {
    let (state, app) = test_state_and_app(CorsConfig::AllowAll).await;
    let chat = here(&state, &[]).await;
    let slots = state.agent_semaphore.available_permits();
    let _held = std::sync::Arc::clone(&state.agent_semaphore)
        .try_acquire_many_owned(u32::try_from(slots).unwrap())
        .unwrap();
    let gone = AttachmentId::of(b"never uploaded").to_string();

    assert_eq!(
        put(&app, "a1", turn(chat, &[])).await,
        (StatusCode::TOO_MANY_REQUESTS, "agent_busy".to_owned())
    );
    assert_eq!(
        put(&app, "a1", turn(chat, &[&gone])).await,
        refusal("attachment_not_found")
    );
}
