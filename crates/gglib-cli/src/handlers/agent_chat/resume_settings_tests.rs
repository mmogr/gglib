//! A resumed chat samples with the profile it was started with (#886).
//!
//! Most tests go through the JSON a conversation row stores, because that is
//! where the profile was lost: the sampling values beside it survived the
//! round trip and the profile had no field to survive in. The last ones run
//! `prepare` over a database, the code that saves the row and reads it back.

use gglib_bootstrap::BootstrapConfig;
use gglib_core::Settings;
use gglib_core::domain::InferenceConfig;
use gglib_core::domain::chat::NewConversation;

use super::super::prepare;
use super::*;
use crate::bootstrap::{CliConfig, CliContext, bootstrap_with};

/// A `ChatArgs` with every knob at rest.
fn chat_args() -> ChatArgs {
    ChatArgs {
        identifier: "qwen".to_owned(),
        context: crate::shared_args::ContextArgs::default(),
        system_prompt: None,
        sampling: crate::shared_args::SamplingArgs::default(),
        retry_policy: gglib_core::retry::RetryPolicy::default(),
        no_tools: false,
        port: None,
        target: Target::Local,
        max_iterations: None,
        tools: Vec::new(),
        tool_timeout_ms: None,
        max_parallel: None,
        verbose: false,
        model: None,
        profile: None,
        continue_id: None,
        observation_tools: Vec::new(),
        max_observation_steps: None,
        max_stagnation_steps: None,
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

fn profiles() -> Vec<InferenceProfile> {
    vec![profile("coding"), profile("chat")]
}

/// What a resume reads back: the settings as the conversation row stores them.
fn stored(settings: &ConversationSettings) -> ConversationSettings {
    let json = serde_json::to_string(settings).unwrap();
    serde_json::from_str(&json).unwrap()
}

#[test]
fn a_chat_started_with_a_profile_resumes_with_it() {
    let saved = stored(&session_settings(&chat_args(), Some(&profile("coding"))));

    let resumed = restore_profile(None, Target::Local, &profiles(), saved.profile.as_deref());

    assert_eq!(resumed.map(|p| p.name), Some("coding".to_owned()));
}

#[test]
fn a_profile_named_on_resume_beats_the_saved_one() {
    let saved = stored(&session_settings(&chat_args(), Some(&profile("coding"))));

    let resumed = restore_profile(
        Some(profile("chat")),
        Target::Local,
        &profiles(),
        saved.profile.as_deref(),
    );

    assert_eq!(resumed.map(|p| p.name), Some("chat".to_owned()));
}

/// A row written before the field existed: its sampling still restores, and
/// it resumes with no profile rather than failing to parse.
#[test]
fn an_old_row_without_the_field_resumes_unprofiled() {
    let saved: ConversationSettings =
        serde_json::from_str(r#"{"model_name":"qwen","top_k":40}"#).unwrap();

    let merged = apply_saved_settings(&chat_args(), &None, &Some(saved.clone()));
    let resumed = restore_profile(None, Target::Local, &profiles(), saved.profile.as_deref());

    assert_eq!(merged.sampling.top_k, Some(40));
    assert!(resumed.is_none());
}

/// A deleted profile must not make the conversation unresumable, the same
/// call `resume_profile` makes for a deleted suffix.
#[test]
fn a_saved_profile_since_deleted_resumes_unprofiled() {
    let saved = stored(&session_settings(&chat_args(), Some(&profile("gone"))));

    let resumed = restore_profile(None, Target::Local, &profiles(), saved.profile.as_deref());

    assert!(resumed.is_none());
}

/// A profile configured here does not describe the paired machine's sampling.
#[test]
fn a_remote_resume_restores_no_profile_from_this_machine() {
    let resumed = restore_profile(None, Target::Remote, &profiles(), Some("coding"));

    assert!(resumed.is_none());
}

#[test]
fn a_chat_started_without_a_profile_saves_none() {
    let saved = stored(&session_settings(&chat_args(), None));

    assert_eq!(saved.profile, None);
    assert!(!serde_json::to_string(&saved).unwrap().contains("profile"));
}

/// The CLI's context over `dir`'s database, with `coding` and `chat`
/// configured.
async fn context(dir: &tempfile::TempDir) -> CliContext {
    let models_dir = dir.path().join("models");
    std::fs::create_dir_all(&models_dir).expect("models dir");
    let ctx = bootstrap_with(
        CliConfig {
            base_port: gglib_core::settings::DEFAULT_LLAMA_BASE_PORT,
            llama_server_path: "/nonexistent/llama-server".into(),
        },
        BootstrapConfig {
            db_path: dir.path().join("gglib.db"),
            llama_server_path: "/nonexistent/llama-server".into(),
            models_dir,
            hf_token: None,
        },
    )
    .await
    .expect("the database opens");
    ctx.settings_repo
        .modify(&|settings: &mut Settings| {
            settings.inference_profiles = Some(profiles());
            Ok(())
        })
        .await
        .expect("profiles saved");
    ctx
}

/// `gglib chat qwen [--profile …]`: the id of the conversation it saves.
async fn start(ctx: &CliContext, profile: Option<&str>) -> i64 {
    let args = ChatArgs {
        profile: profile.map(str::to_owned),
        ..chat_args()
    };
    let session = prepare(ctx, &args).await.expect("a new chat");
    session.persistence.expect("the conversation is saved").id
}

/// `gglib chat --continue <id> [--profile …]`: the profile it samples with.
async fn resume(ctx: &CliContext, id: i64, profile: Option<&str>) -> Option<String> {
    let args = ChatArgs {
        identifier: String::new(),
        continue_id: Some(id),
        profile: profile.map(str::to_owned),
        ..chat_args()
    };
    let session = prepare(ctx, &args).await.expect("the chat resumes");
    session.params.profile.map(|p| p.name)
}

#[tokio::test]
async fn a_chat_started_with_a_profile_continues_with_it() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = context(&dir).await;
    let id = start(&ctx, Some("coding")).await;

    assert_eq!(resume(&ctx, id, None).await.as_deref(), Some("coding"));
}

#[tokio::test]
async fn a_profile_named_on_continue_beats_the_saved_one() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = context(&dir).await;
    let id = start(&ctx, Some("coding")).await;

    assert_eq!(
        resume(&ctx, id, Some("chat")).await.as_deref(),
        Some("chat")
    );
}

/// The row an older gglib saved: no profile field, a sampling value beside
/// it. A `None` profile is skipped when serialized, so this is that JSON.
#[tokio::test]
async fn an_old_row_without_the_field_continues_unprofiled() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = context(&dir).await;
    let saved: ConversationSettings =
        serde_json::from_str(r#"{"model_name":"qwen","top_k":40}"#).unwrap();
    let id = ctx
        .app
        .chat_history()
        .create_conversation_with_settings(NewConversation {
            title: "older".to_owned(),
            model_id: None,
            system_prompt: None,
            settings: Some(saved),
        })
        .await
        .expect("the row is saved");

    let args = ChatArgs {
        identifier: String::new(),
        continue_id: Some(id),
        ..chat_args()
    };
    let session = prepare(&ctx, &args).await.expect("the chat resumes");

    assert_eq!(session.args.sampling.top_k, Some(40), "the row was read");
    assert!(session.params.profile.is_none());
}
