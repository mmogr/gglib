//! `gglib chat` and `gglib q` with an image, on `--port`. For a server
//! that cannot see: refused by name before the agent loop is composed, so
//! the server is asked for its `/props` and nothing else. A file that
//! cannot be attached ends either command before even that. For a server
//! that can: the image reaches it as a data URL, and the saved turn names
//! it.

use std::path::PathBuf;

use super::*;
use crate::bootstrap::test_context;
use crate::handlers::agent_chat::images::images_tests::{file, png};
use crate::handlers::agent_chat::sight::sight_tests::props_server;
use crate::handlers::inference::agent_question::{self, QuestionArgs};

const BLIND: &str = r#"{"modalities":{"vision":false}}"#;

/// `gglib chat qwen --port <port> --image <path>…`.
fn chat_args(port: u16, images: Vec<PathBuf>) -> ChatArgs {
    ChatArgs {
        identifier: "qwen".to_owned(),
        context: ContextArgs::default(),
        system_prompt: None,
        sampling: SamplingArgs::default(),
        retry_policy: gglib_core::retry::RetryPolicy::default(),
        no_tools: true,
        port: Some(port),
        target: Target::Local,
        max_iterations: None,
        tools: Vec::new(),
        tool_timeout_ms: None,
        max_parallel: None,
        images,
        verbose: false,
        model: None,
        profile: None,
        continue_id: None,
        observation_tools: Vec::new(),
        max_observation_steps: None,
        max_stagnation_steps: None,
    }
}

/// `gglib q -m qwen --port <port> --image <path>… "what is this?"`.
fn question_args(port: u16, images: Vec<PathBuf>) -> QuestionArgs {
    QuestionArgs {
        question: "what is this?".to_owned(),
        model_arg: Some("qwen".to_owned()),
        file: None,
        port: Some(port),
        target: Target::Local,
        max_iterations: None,
        tools: vec!["__none__".to_owned()],
        tool_timeout_ms: None,
        max_parallel: None,
        images,
        observation_tools: Vec::new(),
        max_observation_steps: None,
        show_prompt: false,
        verbose: false,
        quiet: true,
        sampling: SamplingArgs::default(),
        profile: None,
        context: ContextArgs::default(),
    }
}

#[tokio::test]
async fn chat_refuses_an_image_for_a_server_that_cannot_see_before_the_loop_is_composed() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    let image = file(&dir, "shot.png", &png(64, 32));
    let server = props_server(BLIND);

    let refused = execute(&ctx, chat_args(server.port, vec![image])).await;

    let refused = refused.expect_err("the server cannot see").to_string();
    assert!(
        refused.starts_with("Model 'qwen' cannot read images"),
        "{refused}"
    );
    assert_eq!(server.requests(), ["GET /props HTTP/1.1"]);
}

#[tokio::test]
async fn q_refuses_an_image_for_a_server_that_cannot_see_before_the_loop_is_composed() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    let image = file(&dir, "shot.png", &png(64, 32));
    let server = props_server(BLIND);

    let refused = agent_question::execute(&ctx, question_args(server.port, vec![image])).await;

    let refused = refused.expect_err("the server cannot see").to_string();
    assert!(
        refused.starts_with("Model 'qwen' cannot read images"),
        "{refused}"
    );
    assert_eq!(server.requests(), ["GET /props HTTP/1.1"]);
}

#[tokio::test]
async fn a_file_that_cannot_be_attached_ends_chat_before_a_conversation_or_a_request() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    let missing = dir.path().join("gone.png");
    let server = props_server(BLIND);

    let refused = execute(&ctx, chat_args(server.port, vec![missing.clone()])).await;

    let refused = refused.expect_err("no such file").to_string();
    assert!(
        refused.contains(&missing.display().to_string()),
        "{refused}"
    );
    assert!(server.requests().is_empty());
    let conversations = ctx.app.chat_history().list_conversations().await.unwrap();
    assert!(conversations.is_empty());
}

#[tokio::test]
async fn a_file_that_cannot_be_attached_ends_q_before_a_request() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    let text = file(&dir, "notes.txt", b"plain text");
    let server = props_server(BLIND);

    let refused =
        agent_question::execute(&ctx, question_args(server.port, vec![text.clone()])).await;

    let refused = refused.expect_err("not an image").to_string();
    assert!(refused.contains(&text.display().to_string()), "{refused}");
    assert!(refused.contains("PNG and JPEG"), "{refused}");
    assert!(server.requests().is_empty());
}

/// The whole path of `gglib q --image`: the file is stored as it is on
/// disk, the request carries it as a data URL after the question, and the
/// saved turn is linked to it.
#[tokio::test]
async fn q_sends_the_image_to_a_server_that_can_see_and_saves_the_turn_with_it() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    let bytes = png(64, 32);
    let image = file(&dir, "shot.png", &bytes);
    let server = props_server(r#"{"modalities":{"vision":true}}"#);

    // `--file`, so the question does not wait on this process's stdin.
    let args = QuestionArgs {
        file: Some(file(&dir, "notes.txt", b"{}").display().to_string()),
        question: "what is this? {}".to_owned(),
        ..question_args(server.port, vec![image])
    };

    agent_question::execute(&ctx, args)
        .await
        .expect("the question is answered");

    let sent: serde_json::Value =
        serde_json::from_str(&server.body_of("POST /v1/chat/completions")).expect("a JSON body");
    let url = gglib_core::domain::attachment::AttachmentBlob {
        mime: "image/png".to_owned(),
        data: bytes.clone(),
    }
    .data_url();
    assert_eq!(
        sent["messages"][1]["content"],
        serde_json::json!([
            {"type": "text", "text": "what is this? {}"},
            {"type": "image_url", "image_url": {"url": url}},
        ])
    );
    let history = ctx.app.chat_history();
    let conversation = history.list_conversations().await.unwrap().remove(0);
    let saved = history.get_messages(conversation.id).await.unwrap();
    let asked = &saved[0].images;
    assert_eq!(asked.len(), 1);
    assert_eq!(
        asked[0].id.as_str(),
        gglib_core::domain::AttachmentId::of(&bytes).as_str()
    );
    assert_eq!((asked[0].width, asked[0].height), (64, 32));
}

/// `gglib chat --continue <id>` with no `--image`: the chat's history is
/// sent again each turn, so a chat that holds an image is refused on a
/// server that cannot see, as a new image would be.
#[tokio::test]
async fn chat_refuses_to_resume_a_chat_that_holds_an_image_on_a_server_that_cannot_see() {
    use gglib_core::domain::chat::{MessageRole, NewConversation, NewMessage};

    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    let stored = ctx.app.attachments().ingest(&png(64, 32)).await.unwrap();
    let history = ctx.app.chat_history();
    let conversation_id = history
        .create_conversation_with_settings(NewConversation {
            title: "Screenshots".to_owned(),
            model_id: None,
            system_prompt: None,
            settings: None,
        })
        .await
        .unwrap();
    history
        .save_message(NewMessage {
            conversation_id,
            role: MessageRole::User,
            content: "what is this?".to_owned(),
            metadata: None,
            images: vec![stored.info.id],
        })
        .await
        .unwrap();
    let server = props_server(BLIND);

    let args = ChatArgs {
        continue_id: Some(conversation_id),
        ..chat_args(server.port, Vec::new())
    };
    let refused = execute(&ctx, args).await;

    let refused = refused.expect_err("the server cannot see").to_string();
    assert!(
        refused.starts_with("Model 'qwen' cannot read images"),
        "{refused}"
    );
    assert_eq!(server.requests(), ["GET /props HTTP/1.1"]);
}
