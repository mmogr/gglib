//! The iteration limit a chat keeps, which the daemon's turns resolve by its
//! rule: the chat's own, then what this machine stores, then the default.
//! A chat keeps only the limit a command line named. `prepare` over a
//! database.

use gglib_core::Settings;
use gglib_core::domain::chat::NewConversation;

use super::super::prepare;
use super::tests::chat_args;
use super::*;
use crate::bootstrap::{CliContext, test_context};

/// The CLI's context over `dir`'s database, storing a limit of 7 and
/// holding `qwen`.
async fn storing(dir: &tempfile::TempDir) -> CliContext {
    let ctx = test_context(dir.path()).await;
    ctx.settings_repo
        .modify(&|settings: &mut Settings| {
            settings.max_tool_iterations = Some(7);
            Ok(())
        })
        .await
        .expect("limits saved");
    let path = dir.path().join("qwen.gguf");
    let model = gglib_core::domain::NewModel::new("qwen".to_owned(), path, 8.0, chrono::Utc::now());
    ctx.app.models().add(model).await.expect("registered");
    ctx
}

/// A stored chat on `qwen` whose settings name `max_iterations`.
async fn chat(ctx: &CliContext, max_iterations: Option<usize>) -> i64 {
    let settings = ConversationSettings {
        model_name: Some("qwen".to_owned()),
        max_iterations,
        ..ConversationSettings::default()
    };
    ctx.app
        .chat_history()
        .create_conversation(NewConversation {
            title: "t".to_owned(),
            settings: Some(settings),
            ..NewConversation::default()
        })
        .await
        .expect("saved")
}

/// The limit conversation `id` keeps as its own.
async fn kept(ctx: &CliContext, id: i64) -> Option<usize> {
    let saved = ctx.app.chat_history().get_conversation(id).await;
    saved.unwrap().unwrap().settings.unwrap().max_iterations
}

/// A new chat keeps as its own only the limit its command line named: one
/// started with none follows this machine's stored limit as it changes,
/// here and on a paired device.
#[tokio::test]
async fn a_new_chat_saves_the_limit_its_command_line_named_and_no_other() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = storing(&dir).await;
    for flag in [None, Some(3)] {
        let args = ChatArgs {
            max_iterations: flag,
            ..chat_args()
        };

        let (_, conversation) = prepare(&ctx, &args).await.expect("a new chat");

        assert_eq!(kept(&ctx, conversation.id).await, flag);
    }
}

/// A resume keeps the limit the chat saved, or none, unless its command
/// line names one, which the chat then keeps in its place.
#[tokio::test]
async fn a_resume_keeps_the_chats_limit_unless_it_names_one() {
    for (saved, flag, want) in [
        (Some(4), None, Some(4)),
        (None, None, None),
        (Some(4), Some(3), Some(3)),
        (None, Some(3), Some(3)),
    ] {
        let dir = tempfile::tempdir().expect("tempdir");
        let ctx = storing(&dir).await;
        let id = chat(&ctx, saved).await;
        let args = ChatArgs {
            identifier: String::new(),
            continue_id: Some(id),
            max_iterations: flag,
            ..chat_args()
        };

        prepare(&ctx, &args).await.expect("resumed");

        assert_eq!(kept(&ctx, id).await, want, "saved {saved:?}, flag {flag:?}");
    }
}
