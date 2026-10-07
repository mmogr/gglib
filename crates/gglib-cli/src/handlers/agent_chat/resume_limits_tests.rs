//! The limits a `gglib chat` session runs with, by the rule the daemon
//! resolves a turn's with: the flag, then the limit the chat saved, then
//! what this machine stores, then the default. `prepare` over a database.

use gglib_core::Settings;
use gglib_core::domain::agent::{DEFAULT_MAX_ITERATIONS, TurnLimits};
use gglib_core::domain::chat::NewConversation;

use super::super::prepare;
use super::tests::chat_args;
use super::*;
use crate::bootstrap::{CliContext, test_context};

/// The CLI's context over `dir`'s database, storing these two limits.
async fn storing(dir: &tempfile::TempDir, iterations: Option<u32>, stagnation: u32) -> CliContext {
    let ctx = test_context(dir.path()).await;
    ctx.settings_repo
        .modify(&|settings: &mut Settings| {
            settings.max_tool_iterations = iterations;
            settings.max_stagnation_steps = Some(stagnation);
            Ok(())
        })
        .await
        .expect("limits saved");
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

/// Each row is what this machine stores, the limit the chat saved (`None`
/// for a new chat, `Some(None)` for a resumed one that saved none),
/// `--max-iterations`, and the limit the session runs with.
#[tokio::test]
async fn a_session_runs_with_the_flag_then_the_chats_limit_then_the_stored_one() {
    let table = [
        (Some(7), None, None, 7),
        (Some(7), None, Some(3), 3),
        (Some(7), Some(None), None, 7),
        (Some(7), Some(Some(4)), None, 4),
        (Some(7), Some(Some(4)), Some(3), 3),
        (None, None, None, DEFAULT_MAX_ITERATIONS),
        (None, Some(Some(4)), None, 4),
    ];
    for (stored, saved, flag, want) in table {
        let dir = tempfile::tempdir().expect("tempdir");
        let ctx = storing(&dir, stored, 9).await;
        let continue_id = match saved {
            Some(max_iterations) => Some(chat(&ctx, max_iterations).await),
            None => None,
        };
        let args = ChatArgs {
            identifier: continue_id.map_or_else(|| "qwen".to_owned(), |_| String::new()),
            continue_id,
            max_iterations: flag,
            ..chat_args()
        };

        let session = prepare(&ctx, &args).await.expect("a session");

        let limits = TurnLimits {
            max_iterations: want,
            max_stagnation_steps: Some(9),
        };
        assert_eq!(
            session.limits, limits,
            "stored {stored:?}, saved {saved:?}, flag {flag:?}"
        );
    }
}

/// A new chat keeps as its own only the limit its command line named: one
/// started with none follows this machine's stored limit as it changes,
/// here and on a paired device.
#[tokio::test]
async fn a_new_chat_saves_the_limit_its_command_line_named_and_no_other() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = storing(&dir, Some(7), 9).await;
    for flag in [None, Some(3)] {
        let args = ChatArgs {
            max_iterations: flag,
            ..chat_args()
        };

        let session = prepare(&ctx, &args).await.expect("a new chat");

        let id = session.persistence.expect("the conversation is saved").id;
        assert_eq!(kept(&ctx, id).await, flag);
    }
}
