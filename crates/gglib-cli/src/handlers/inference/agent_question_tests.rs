//! `gglib q` against a stand-in daemon: the chat it makes keeps what its
//! command line named, and its one turn says the folder it was asked in.
//! Which folders a question adopts is `agent_chat::files`' rule and tests.
//! A question that names no model asks the default, else the library's only
//! chat model, and says which.

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

/// Ask the question of a stand-in daemon that answers: the chat it made,
/// and the body of the turn it sent.
async fn asked(no_tools: bool) -> (gglib_core::domain::chat::Conversation, Value) {
    asked_with(no_tools, &[]).await
}

/// [`asked`], with `--tools` naming `tools`.
async fn asked_with(
    no_tools: bool,
    tools: &[&str],
) -> (gglib_core::domain::chat::Conversation, Value) {
    gglib_core::paths::isolate_data_root();
    let dir = tempfile::tempdir().expect("tempdir");
    let notes = dir.path().join("notes.txt");
    std::fs::write(&notes, "a cat on a mat").unwrap();
    let args = QuestionArgs {
        tools: tools.iter().map(|name| (*name).to_owned()).collect(),
        ..question(no_tools, &notes)
    };
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

/// The directory a test runs in, its crate's, canonical: neither home nor a
/// system folder, so a question asked there reads it.
fn canonical_here() -> String {
    let here = std::env::current_dir().unwrap();
    let here = gglib_core::paths::canonical(here).unwrap();
    here.display().to_string()
}

/// A question whose file tools read no folder is told of no file tool and
/// no folder, though it keeps its other tools; one with a folder is told of
/// both.
#[test]
fn a_question_that_reads_no_folder_is_not_told_of_file_tools() {
    let told = system_prompt(false, None);
    for named in [
        "read_file",
        "list_directory",
        "grep_search",
        "Working directory",
    ] {
        assert!(!told.contains(named), "{named} in: {told}");
    }
    assert!(told.contains("cannot read files"), "{told}");
    assert!(!told.contains("no tools"), "{told}");

    let told = system_prompt(false, Some("/work/api"));
    assert!(told.contains("read_file"), "{told}");
    assert!(told.contains("Working directory: /work/api"), "{told}");
}

/// A `--tools` list that names no file tool is honoured as typed, as
/// `gglib chat`'s is: the turn says no folder, so the daemon adds no file
/// tool to the list, and the model is told of none.
#[tokio::test]
async fn a_question_whose_tools_name_no_file_tool_reads_no_folder() {
    let (chat, body) = asked_with(false, &["tavily_search"]).await;

    assert_eq!(body["folder"], Value::Null);
    let told = chat.system_prompt.unwrap_or_default();
    assert!(!told.contains("read_file"), "{told}");
    assert!(told.contains("cannot read files"), "{told}");
}

/// The question's turn says the directory it was asked in as its folder,
/// canonical, and the question as its words.
#[tokio::test]
async fn the_questions_turn_says_the_folder_it_was_asked_in() {
    let here = canonical_here();

    let (chat, body) = asked(false).await;

    assert_eq!(body["conversation_id"], json!(chat.id));
    let content = body["content"].as_str().unwrap_or_default();
    assert!(content.contains("what is this?"), "{content}");
    assert!(content.contains("a cat on a mat"), "{content}");
    assert_eq!(body["folder"], json!(here));
}

/// A question asked with `--no-tools` is told of no tool and no folder, and
/// one asked with its tools is told of both. It is not told it has no tools
/// at all: its chat can go on to a message sent with `/draw`, which is given
/// the image tool.
#[tokio::test]
async fn a_question_without_tools_is_not_told_it_has_them() {
    let here = canonical_here();
    let (plain, _) = asked(false).await;
    let (toolless, _) = asked(true).await;

    let told = plain.system_prompt.unwrap_or_default();
    assert!(told.contains("read_file"), "{told}");
    assert!(told.contains(&here), "{told}");

    let told = toolless.system_prompt.unwrap_or_default();
    for named in [
        "read_file",
        "list_directory",
        "grep_search",
        "codebase",
        &here,
    ] {
        assert!(!told.contains(named), "{named} in: {told}");
    }
    assert!(told.contains("cannot read files"), "{told}");
    assert!(!told.to_lowercase().contains("no tools"), "{told}");
}

/// A context over a fresh library in `dir` that holds chat models named
/// `chat` and image models named `image`, in that order, and no default.
async fn library_of(dir: &std::path::Path, chat: &[&str], image: &[&str]) -> CliContext {
    gglib_core::paths::isolate_data_root();
    let ctx = test_context(dir).await;
    let named = |name: &str| {
        let path = dir.join(format!("{name}.gguf"));
        gglib_core::domain::NewModel::new(name.to_owned(), path, Some(8.0), chrono::Utc::now())
    };
    for name in chat {
        ctx.app.models().add(named(name)).await.expect("registered");
    }
    for name in image {
        let mut model = named(name);
        model.image_family = Some(gglib_core::domain::ImageFamily::Flux1);
        ctx.app.models().add(model).await.expect("registered");
    }
    ctx
}

/// The model a question that named none asks in `ctx`, by name, and what it
/// said of the choice.
async fn unnamed(ctx: &CliContext, quiet: bool) -> Result<(String, String)> {
    let default_id = ctx.app.settings().get().await.unwrap().default_model_id;
    let mut said = Vec::new();
    let id = unnamed_model(ctx, default_id, quiet, &mut said).await?;
    let model = ctx
        .app
        .models()
        .get(&id)
        .await
        .unwrap()
        .expect("in the library");
    Ok((model.name, String::from_utf8(said).unwrap()))
}

/// With no default, a library of one chat model answers a question that
/// names none, and the question says which model that is, unless `--quiet`.
/// Image models are not counted.
#[tokio::test]
async fn a_question_that_names_no_model_asks_the_only_chat_model_and_says_which() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = library_of(dir.path(), &["qwen"], &[]).await;
    assert_eq!(
        unnamed(&ctx, false).await.unwrap(),
        (
            "qwen".to_owned(),
            "Model: qwen (the only chat model in the library)\n".to_owned()
        )
    );
    assert_eq!(
        unnamed(&ctx, true).await.unwrap(),
        ("qwen".to_owned(), String::new())
    );

    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = library_of(dir.path(), &["gemma"], &["flux", "sdxl"]).await;
    assert_eq!(unnamed(&ctx, true).await.unwrap().0, "gemma");
}

/// With no default and no chat model, or several, there is nothing to
/// choose by: the question says to name a model or set a default.
#[tokio::test]
async fn a_question_that_names_no_model_is_refused_with_no_chat_model_or_several() {
    for (chat, image) in [
        (&[][..], &[][..]),
        (&[][..], &["flux"][..]),
        (&["qwen", "gemma"][..], &[][..]),
    ] {
        let dir = tempfile::tempdir().expect("tempdir");
        let ctx = library_of(dir.path(), chat, image).await;

        let refused = unnamed(&ctx, false)
            .await
            .expect_err("nothing to choose by");

        let said = refused.to_string();
        assert!(
            said.starts_with("No model specified and no default model set."),
            "{chat:?} {image:?}: {said}"
        );
        assert!(said.contains("gglib config set default-model-id"), "{said}");
    }
}

/// A default the settings name is asked, among several, and nothing is
/// said of it: the user chose it.
#[tokio::test]
async fn a_question_that_names_no_model_asks_the_default_and_says_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = library_of(dir.path(), &["qwen", "gemma"], &[]).await;
    let gemma = ctx.app.models().get("gemma").await.unwrap().unwrap();
    let update = gglib_core::settings::SettingsUpdate {
        default_model_id: Some(Some(gemma.id)),
        ..Default::default()
    };
    ctx.app.settings().update(update).await.expect("saved");

    assert_eq!(
        unnamed(&ctx, false).await.unwrap(),
        ("gemma".to_owned(), String::new())
    );
}

/// The whole command, with no `-m`, in a library of one chat model and no
/// default: the question is answered, on a chat that names that model.
#[tokio::test]
async fn a_question_with_no_model_named_is_answered_by_the_only_chat_model() {
    let dir = tempfile::tempdir().expect("tempdir");
    let notes = dir.path().join("notes.txt");
    std::fs::write(&notes, "a cat on a mat").unwrap();
    let ctx = library_of(dir.path(), &["qwen"], &["flux"]).await;
    let args = QuestionArgs {
        model_arg: None,
        ..question(true, &notes)
    };
    let (port, _taken) = stand_in(Answer::Runs {
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
    let named = chat.settings.as_ref().and_then(|s| s.model_name.clone());
    assert_eq!(named.as_deref(), Some("qwen"), "{chat:?}");
}
