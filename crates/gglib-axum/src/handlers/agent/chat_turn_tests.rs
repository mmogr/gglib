//! A chat run with builtins: an `OpenAI` request read as the loop's, its
//! inline images stored once, the image tool only with Draw, the request's
//! sampling not read but its Thinking choice honoured, nothing written to
//! any chat, and a run in the device's scope that says its events are the
//! agent loop's.

use gglib_core::domain::agent::AgentMessage;
use gglib_core::domain::runs::{RunFrames, RunKind, RunStatus};
use gglib_core::domain::{AttachmentBlob, AttachmentId};
use gglib_core::ports::{RunScope, RunsError, RunsPort as _};
use serde_json::{Value, json};

use super::{read, start};
use crate::error::HttpError;
use crate::handlers::agent::image_gate::image_gate_tests::model;
use crate::handlers::agent::run_fixture::{drawing_state, settled, state};
use crate::handlers::agent::turn_fixture::{self, sent};

/// A PNG's signature and `IHDR`, 64 by 64.
fn png() -> Vec<u8> {
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    bytes.extend([0, 0, 0, 13]);
    bytes.extend(b"IHDR");
    bytes.extend(64_u32.to_be_bytes());
    bytes.extend(64_u32.to_be_bytes());
    bytes.extend([8, 6, 0, 0, 0, 0, 0, 0, 0]);
    bytes
}

/// A phone's request on `model`: a system prompt, a picture with a
/// question, an earlier drawing, and the new message.
fn request(model: &str) -> Value {
    let picture = AttachmentBlob {
        mime: "image/png".to_owned(),
        data: png(),
    }
    .data_url();
    json!({
        "model": model,
        "temperature": 1.9,
        "stream": true,
        "messages": [
            { "role": "system", "content": "Be brief." },
            { "role": "user", "content": [
                { "type": "text", "text": "what is this?" },
                { "type": "image_url", "image_url": { "url": picture } },
            ] },
            { "role": "assistant", "content": null, "tool_calls": [
                { "id": "c1", "type": "function", "function": {
                    "name": "builtin:generate_image", "arguments": "{\"prompt\":\"a fox\"}" } } ] },
            { "role": "tool", "tool_call_id": "c1", "content": "Drew 1 image." },
            { "role": "assistant", "content": "I drew a fox." },
            { "role": "user", "content": "now a red one" },
        ],
    })
}

fn device(name: &str) -> RunScope {
    RunScope::Device(name.to_owned())
}

/// The refusal's status and code.
fn refused<T>(result: Result<T, HttpError>) -> (u16, &'static str) {
    match result {
        Err(HttpError::Coded { status, code, .. }) => (status.as_u16(), code),
        Err(other) => panic!("uncoded: {other}"),
        Ok(_) => panic!("not refused"),
    }
}

fn offered(body: &Value) -> Vec<String> {
    let tools = body["tools"].as_array().cloned().unwrap_or_default();
    tools
        .iter()
        .map(|tool| tool["function"]["name"].as_str().unwrap().to_owned())
        .collect()
}

/// The server the loop is run against: one whose model calls tools and can
/// see. Its id.
async fn seeing_server(state: &crate::state::AppState) -> i64 {
    let served = turn_fixture::model(state, |_| {}).await;
    let models = state.core.models();
    let mut seeing = models.get_by_id(served).await.unwrap().unwrap();
    seeing.projector_path = Some("models/mmproj-F16.gguf".into());
    models.update(&seeing).await.unwrap();
    served
}

/// The history becomes the loop's messages, the inline image stored and
/// named by its id; the model is offered the image tool only with Draw,
/// when its first reply must call it, and what the request says of sampling
/// is not sent on.
#[tokio::test]
async fn an_openai_request_is_read_as_the_loops_with_the_image_tool_only_for_draw() {
    let (_dir, state) = drawing_state().await;
    model(&state, "qwen-vision", true).await;
    let served = seeing_server(&state).await;
    let picture = AttachmentId::of(&png());

    let (on, drawn) = read(&state, &request("qwen-vision"), true).await.unwrap();

    assert_eq!(on, "qwen-vision");
    assert!(drawn.draw);
    assert_eq!(drawn.messages.len(), 6);
    assert!(matches!(
        &drawn.messages[1],
        AgentMessage::User { content, images } if content == "what is this?" && images == std::slice::from_ref(&picture)
    ));
    assert!(
        matches!(&drawn.messages[3], AgentMessage::Tool { tool_call_id, .. } if tool_call_id == "c1")
    );
    assert!(state.core.attachments().info(&picture).await.is_ok());
    let [body, later] = turn_fixture::drawn(&state, served, drawn).await;
    assert_eq!(offered(&body), ["builtin:generate_image"]);
    assert_eq!(body["tool_choice"], "required", "Draw must draw");
    assert_eq!(offered(&later), ["builtin:generate_image"]);
    assert_eq!(later["tool_choice"], "auto");
    assert_ne!(
        body["temperature"],
        json!(1.9),
        "the request's sampling is not read"
    );

    let (_, plain) = read(&state, &request("qwen-vision"), false).await.unwrap();
    assert!(!plain.draw);
    let body = sent(&state, served, plain).await;
    assert_eq!(offered(&body), Vec::<String>::new(), "no tool without Draw");
}

/// A phone that turned Thinking off says so as it does on the chat route,
/// with a budget of zero, and the run thinks off too.
#[tokio::test]
async fn a_request_that_turns_thinking_off_runs_with_no_thinking_budget() {
    let (_dir, state) = state().await;
    model(&state, "qwen-vision", true).await;
    let served = seeing_server(&state).await;
    let mut off = request("qwen-vision");
    off["reasoning_budget_tokens"] = json!(0);

    let (_, chat) = read(&state, &off, false).await.unwrap();

    assert_eq!(chat.reasoning_budget_tokens, Some(0));
    assert_eq!(chat.reasoning_effort, None);
    let body = sent(&state, served, chat).await;
    assert_eq!(body["reasoning_budget_tokens"], json!(0));
}

/// Nothing else of the request's sampling is read: a thinking budget above
/// zero, an effort and a token limit are the phone's sampling, and a body
/// that says nothing of thinking leaves it to the model.
#[tokio::test]
async fn a_thinking_budget_above_zero_is_sampling_and_is_not_read() {
    let (_dir, state) = state().await;
    model(&state, "qwen-vision", true).await;
    let served = seeing_server(&state).await;
    let mut sampled = request("qwen-vision");
    sampled["reasoning_budget_tokens"] = json!(4096);
    sampled["reasoning_effort"] = json!("high");
    sampled["max_tokens"] = json!(5);

    let (_, chat) = read(&state, &sampled, false).await.unwrap();

    assert_eq!(chat.reasoning_budget_tokens, None);
    assert_eq!(chat.reasoning_effort, None);
    let body = sent(&state, served, chat).await;
    assert_ne!(body["reasoning_budget_tokens"], json!(4096));
    assert_ne!(body["reasoning_budget_tokens"], json!(0));
    assert_ne!(body["reasoning_effort"], json!("high"));
    assert_ne!(body["max_tokens"], json!(5));

    let (_, unsaid) = read(&state, &request("qwen-vision"), false).await.unwrap();
    assert_eq!(unsaid.reasoning_budget_tokens, None);
}

/// The same picture sent with every turn is one attachment.
#[tokio::test]
async fn an_inline_image_sent_again_is_stored_once() {
    let (_dir, state) = state().await;
    model(&state, "qwen-vision", true).await;
    for _ in 0..2 {
        read(&state, &request("qwen-vision"), false).await.unwrap();
    }
    let picture = AttachmentId::of(&png());
    let stored = state
        .core
        .attachments()
        .ids_starting_with(&picture.as_str()[..8])
        .await
        .unwrap();
    assert_eq!(stored, [picture]);
}

/// The run is a chat run in the device's scope whose events are the agent
/// loop's, as its listing says; it is saved to no chat, and another device
/// cannot reach it. A retry of the id finds it.
#[tokio::test]
async fn the_run_is_the_devices_says_agent_frames_and_writes_no_rows() {
    let (_dir, state) = state().await;
    model(&state, "qwen-vision", true).await;

    let created = start(&state, "phone", "p1", request("qwen-vision"), false)
        .await
        .unwrap();

    assert!(created.created);
    let info = created.info;
    assert_eq!((info.kind, info.frames), (RunKind::Chat, RunFrames::Agent));
    assert_eq!(info.device.as_deref(), Some("phone"));
    assert_eq!(info.conversation_id, None);
    assert_eq!(info.model.as_deref(), Some("qwen-vision"));
    let listed = state.runs.list(&device("phone")).runs;
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].frames, RunFrames::Agent);
    let wire = serde_json::to_value(&listed[0]).unwrap();
    assert_eq!(wire["frames"], "agent");
    assert!(state.runs.list(&device("laptop")).runs.is_empty());
    assert!(matches!(
        state.runs.existing(&device("laptop"), "p1"),
        Err(RunsError::IdTaken)
    ));
    let again = start(&state, "phone", "p1", request("qwen-vision"), false)
        .await
        .unwrap();
    assert!(!again.created);

    // Nothing serves this harness's model, so the run ends as a hub turn's
    // would, and nothing was written on the way.
    settled(&state).await;
    let run = state
        .runs
        .existing(&device("phone"), "p1")
        .unwrap()
        .unwrap();
    assert_eq!(run.status, RunStatus::Failed);
    assert_eq!(run.error.unwrap().code, "model_unavailable");
    let chats = state
        .core
        .chat_history()
        .list_conversations()
        .await
        .unwrap();
    assert!(chats.is_empty(), "no conversation, no row");
}

/// What cannot run is refused before a run is made: no model, a history
/// that does not read (named by its index), a model that draws, Draw where
/// nothing can draw, and an image for a model that cannot see.
#[tokio::test]
async fn what_cannot_run_is_refused_before_a_run_is_made() {
    let (_dir, state) = state().await;
    model(&state, "qwen-vision", true).await;
    model(&state, "qwen-blind", false).await;
    let refusal = |body: Value, draw: bool| {
        let state = state.clone();
        async move { refused(start(&state, "phone", "p1", body, draw).await) }
    };

    let mut unnamed = request("qwen-vision");
    unnamed.as_object_mut().unwrap().remove("model");
    assert_eq!(refusal(unnamed, false).await, (400, "invalid_request"));

    let odd = json!({ "model": "qwen-vision", "messages": [
        { "role": "user", "content": "hi" }, { "role": "narrator", "content": "SECRET" } ] });
    match start(&state, "phone", "p1", odd, false).await {
        Err(HttpError::Coded { code, message, .. }) => {
            assert_eq!(code, "invalid_request");
            assert!(
                message.contains("message 1") && !message.contains("SECRET"),
                "{message}"
            );
        }
        other => panic!("not a coded refusal: {other:?}"),
    }

    assert_eq!(
        refusal(request("qwen-vision"), true).await,
        (400, "drawing_unavailable")
    );
    assert_eq!(
        refusal(request("qwen-blind"), false).await,
        (400, "model_cannot_read_images")
    );
    assert!(state.runs.list(&device("phone")).runs.is_empty());
    assert_eq!(state.agent_semaphore.available_permits(), 1);
}
