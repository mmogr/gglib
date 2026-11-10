//! What a chat saves of its command line, which the daemon reads on each of
//! its turns: a new chat saves the flags it names, and a resume keeps what
//! the chat saved with what it names in its place. Which machine it resumes
//! on is `resume_machine_tests`'.
//!
//! Each test runs `prepare` over a database of its own and reads the row it
//! saved.

use gglib_core::Settings;
use gglib_core::domain::chat::NewConversation;
use gglib_core::domain::{InferenceConfig, Machine, ModelRef};

use super::super::prepare;
use super::*;
use crate::bootstrap::{CliContext, test_context};
use crate::target::Target;

/// A `ChatArgs` with every knob at rest, on `qwen`.
pub(crate) fn chat_args() -> ChatArgs {
    ChatArgs {
        identifier: "qwen".to_owned(),
        system_prompt: None,
        sampling: crate::shared_args::SamplingArgs::default(),
        no_tools: false,
        target: Target::Local,
        max_iterations: None,
        tools: Vec::new(),
        tool_timeout_ms: None,
        max_parallel: None,
        images: Vec::new(),
        verbose: false,
        profile: None,
        continue_id: None,
        thinking: None,
    }
}

fn profile(name: &str) -> InferenceProfile {
    InferenceProfile {
        name: name.to_owned(),
        description: None,
        config: InferenceConfig::default(),
        list_in_models: false,
    }
}

/// The CLI's context over `dir`'s database, with `coding` and `chat`
/// configured and `qwen` in the catalogue: the context, and qwen's id.
async fn context(dir: &tempfile::TempDir) -> (CliContext, i64) {
    let ctx = test_context(dir.path()).await;
    ctx.settings_repo
        .modify(&|settings: &mut Settings| {
            settings.inference_profiles = Some(vec![profile("coding"), profile("chat")]);
            Ok(())
        })
        .await
        .expect("profiles saved");
    let path = dir.path().join("qwen.gguf");
    let model =
        gglib_core::domain::NewModel::new("qwen".to_owned(), path, Some(8.0), chrono::Utc::now());
    let qwen = ctx.app.models().add(model).await.expect("registered").id;
    (ctx, qwen)
}

/// `args`, prepared: the id of the conversation it saves to.
async fn prepared(ctx: &CliContext, args: &ChatArgs) -> i64 {
    let (_, conversation) = prepare(ctx, args).await.expect("a session");
    conversation.id
}

/// The settings chat `id` saved.
async fn saved(ctx: &CliContext, id: i64) -> ConversationSettings {
    let row = ctx.app.chat_history().get_conversation(id).await;
    let row = row.expect("read").expect("there");
    row.settings.expect("settings saved")
}

/// `gglib chat --continue <id>`, with nothing else named.
fn resume(id: i64) -> ChatArgs {
    ChatArgs {
        identifier: String::new(),
        continue_id: Some(id),
        ..chat_args()
    }
}

/// A new chat saves its model by id and every flag its command line named:
/// sampling, tools, limits and the profile.
#[tokio::test]
async fn a_new_chat_saves_the_flags_it_names() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (ctx, qwen) = context(&dir).await;
    let mut args = ChatArgs {
        profile: Some("coding".to_owned()),
        tools: vec!["read_file".to_owned()],
        max_iterations: Some(7),
        tool_timeout_ms: Some(9_000),
        max_parallel: Some(2),
        ..chat_args()
    };
    args.sampling.temperature = Some(0.4);
    args.sampling.top_k = Some(40);

    let settings = saved(&ctx, prepared(&ctx, &args).await).await;

    let here = ModelRef {
        machine: Machine::Local,
        id: qwen,
    };
    assert_eq!(settings.model, Some(here));
    assert_eq!(settings.model_name.as_deref(), Some("qwen"));
    assert_eq!(
        (settings.temperature, settings.top_k),
        (Some(0.4), Some(40))
    );
    assert_eq!(settings.profile.as_deref(), Some("coding"));
    assert_eq!(settings.tools, ["read_file"]);
    assert_eq!(
        (
            settings.max_iterations,
            settings.tool_timeout_ms,
            settings.max_parallel
        ),
        (Some(7), Some(9_000), Some(2))
    );
    assert_eq!(settings.no_tools, None);
}

/// A resume keeps what the chat saved and saves what its command line names
/// in its place: a sampling flag, `--no-tools` and a profile.
#[tokio::test]
async fn a_resume_keeps_what_the_chat_saved_with_what_it_names_in_its_place() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (ctx, _) = context(&dir).await;
    let mut started = ChatArgs {
        profile: Some("coding".to_owned()),
        max_iterations: Some(7),
        ..chat_args()
    };
    started.sampling.temperature = Some(0.4);
    started.sampling.top_k = Some(40);
    let id = prepared(&ctx, &started).await;

    let mut again = ChatArgs {
        profile: Some("chat".to_owned()),
        no_tools: true,
        ..resume(id)
    };
    again.sampling.temperature = Some(0.9);
    prepared(&ctx, &again).await;

    let settings = saved(&ctx, id).await;
    assert_eq!(settings.temperature, Some(0.9), "the flag named");
    assert_eq!(settings.top_k, Some(40), "kept");
    assert_eq!(settings.max_iterations, Some(7), "kept");
    assert_eq!(settings.profile.as_deref(), Some("chat"));
    assert_eq!(settings.no_tools, Some(true));
}

/// A resume that names nothing leaves the chat's row as it was.
#[tokio::test]
async fn a_resume_that_names_nothing_writes_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (ctx, _) = context(&dir).await;
    let mut started = chat_args();
    started.sampling.temperature = Some(0.4);
    let id = prepared(&ctx, &started).await;
    let before = ctx.app.chat_history().get_conversation(id).await.unwrap();

    prepared(&ctx, &resume(id)).await;

    let after = ctx.app.chat_history().get_conversation(id).await.unwrap();
    let stamp = |row: Option<gglib_core::domain::chat::Conversation>| {
        row.map(|row| (row.settings, row.updated_at))
    };
    assert_eq!(stamp(after), stamp(before));
}

/// The row an older gglib saved names its model only by name, with a
/// sampling value beside it: it resumes on that model, now saved by id,
/// and keeps the value.
#[tokio::test]
async fn an_old_row_that_names_its_model_resumes_on_it_by_id() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (ctx, qwen) = context(&dir).await;
    let older: ConversationSettings =
        serde_json::from_str(r#"{"model_name":"qwen","top_k":40}"#).unwrap();
    let id = ctx
        .app
        .chat_history()
        .create_conversation(NewConversation {
            title: "older".to_owned(),
            model_id: None,
            system_prompt: None,
            settings: Some(older),
        })
        .await
        .expect("the row is saved");

    let (args, _) = prepare(&ctx, &resume(id)).await.expect("the chat resumes");

    assert_eq!(args.identifier, "qwen");
    let settings = saved(&ctx, id).await;
    assert_eq!(settings.top_k, Some(40));
    assert_eq!(settings.model.map(|m| m.id), Some(qwen));
}

/// A model this catalogue does not hold is refused before any row is
/// saved: the daemon would answer the chat with another model.
#[tokio::test]
async fn a_model_this_catalogue_does_not_hold_is_refused_before_a_chat_is_made() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (ctx, _) = context(&dir).await;
    let args = ChatArgs {
        identifier: "nope".to_owned(),
        ..chat_args()
    };

    let refused = prepare(&ctx, &args).await.err().expect("refused");

    assert_eq!(
        refused.to_string(),
        "No model found matching: 'nope'\nUse 'gglib model list' to see available models."
    );
    let chats = ctx.app.chat_history().list_conversations().await.unwrap();
    assert!(chats.is_empty(), "{chats:?}");
}

/// `--system-prompt` on a new chat is its prompt, and on a resume replaces
/// the one it saved, which every later turn is sent with. Without it a
/// resume keeps the chat's.
#[tokio::test]
async fn a_system_prompt_named_is_the_chats_from_then_on() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (ctx, _) = context(&dir).await;
    let prompt = async |id| {
        let row = ctx.app.chat_history().get_conversation(id).await.unwrap();
        row.expect("there").system_prompt
    };
    let started = ChatArgs {
        system_prompt: Some("Be brief.".to_owned()),
        ..chat_args()
    };
    let id = prepared(&ctx, &started).await;
    assert_eq!(prompt(id).await.as_deref(), Some("Be brief."));

    prepared(&ctx, &resume(id)).await;
    assert_eq!(prompt(id).await.as_deref(), Some("Be brief."));

    let renamed = ChatArgs {
        system_prompt: Some("Be long.".to_owned()),
        ..resume(id)
    };
    prepared(&ctx, &renamed).await;
    assert_eq!(prompt(id).await.as_deref(), Some("Be long."));
}
