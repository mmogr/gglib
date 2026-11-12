//! `gglib q` against a stand-in daemon: the chat it makes keeps what its
//! command line named, and its one turn says the folder it was asked in.

use serde_json::{Value, json};

use super::*;
use crate::bootstrap::test_context;
use crate::daemon_client::STAND_IN_PORT;
use crate::handlers::agent_chat::turn::turn_stand_in::{Answer, stand_in};
use gglib_core::domain::runs::RunStatus;

/// `gglib q --quiet -m qwen [--no-tools] --file <notes> "what is this?"`: a
/// file, so that nothing is read from the test's stdin.
fn question(no_tools: bool, notes: &std::path::Path) -> QuestionArgs {
    QuestionArgs {
        question: "what is this?".to_owned(),
        model_arg: Some("qwen".to_owned()),
        file: Some(notes.display().to_string()),
        target: Target::Local,
        max_iterations: None,
        tools: Vec::new(),
        no_tools,
        tool_timeout_ms: None,
        max_parallel: None,
        images: Vec::new(),
        show_prompt: false,
        verbose: false,
        quiet: true,
        sampling: ChatSamplingArgs::default(),
        profile: None,
    }
}

/// Ask `args` of a stand-in daemon that answers: the chat it made, and the
/// body of the turn it sent.
async fn asked(no_tools: bool) -> (gglib_core::domain::chat::Conversation, Value) {
    gglib_core::paths::isolate_data_root();
    let dir = tempfile::tempdir().expect("tempdir");
    let notes = dir.path().join("notes.txt");
    std::fs::write(&notes, "a cat on a mat").unwrap();
    let args = question(no_tools, &notes);
    let ctx = test_context(dir.path()).await;
    let path = dir.path().join("qwen.gguf");
    let model =
        gglib_core::domain::NewModel::new("qwen".to_owned(), path, Some(8.0), chrono::Utc::now());
    ctx.app.models().add(model).await.expect("registered");
    let (port, taken) = stand_in(Answer::Runs {
        frames: vec![json!({"type": "final_answer", "content": "A cat."}).to_string()],
        status: RunStatus::Completed,
        error: None,
    });

    STAND_IN_PORT
        .scope(port, execute(&ctx, args))
        .await
        .expect("answered");

    let chats = ctx.app.chat_history().list_conversations().await.unwrap();
    let [chat] = chats.as_slice() else {
        panic!("one chat: {chats:?}");
    };
    let put = taken
        .lock()
        .unwrap()
        .iter()
        .find(|(line, _)| line.starts_with("PUT "))
        .map(|(_, body)| body.clone())
        .expect("the turn was sent");
    (chat.clone(), serde_json::from_str(&put).unwrap())
}

/// `--no-tools` is saved with the question's chat, which the daemon reads,
/// and a question without it saves no such setting.
#[tokio::test]
async fn no_tools_is_saved_with_the_questions_chat() {
    let (plain, _) = asked(false).await;
    let (toolless, _) = asked(true).await;

    let saved = |chat: &gglib_core::domain::chat::Conversation| {
        chat.settings.as_ref().and_then(|s| s.no_tools)
    };
    assert_eq!(saved(&plain), None);
    assert_eq!(saved(&toolless), Some(true));
}

/// The question's turn says the directory it was asked in as its folder,
/// and the question as its words.
#[tokio::test]
async fn the_questions_turn_says_the_folder_it_was_asked_in() {
    let here = std::env::current_dir().unwrap().display().to_string();

    let (chat, body) = asked(false).await;

    assert_eq!(body["conversation_id"], json!(chat.id));
    let content = body["content"].as_str().unwrap_or_default();
    assert!(content.contains("what is this?"), "{content}");
    assert!(content.contains("a cat on a mat"), "{content}");
    assert_eq!(body["folder"], json!(here));
}
