//! `gglib chat --continue <id>` while the daemon is replying to that chat,
//! for the page or a paired device: refused, by the listing the daemon gives
//! of its runs, before the session stores anything.
//!
//! Each test runs `run` with a stand-in on a loopback port in the daemon's
//! place. The chat holds an image and its `--port` server cannot see, so a
//! session that goes on past the question ends there, at a refusal that asks
//! the server its `/props`, and never at a prompt.

use gglib_core::domain::Thinking;
use gglib_core::domain::attachment::AttachmentId;
use gglib_core::domain::chat::{MessageRole, NewConversation, NewMessage};
use gglib_core::domain::runs::{RunInfo, RunKind, RunList, RunStatus};
use serde_json::{Value, json};

use super::images::images_tests::{file, png};
use super::resume_settings::tests::chat_args;
use super::sight::sight_tests::props_server;
use super::*;
use crate::bootstrap::test_context;
use crate::daemon_client::handle_tests::{Asked, answering, daemon_health, nobody};
use crate::daemon_client::{STAND_IN_PORT, paths};
use crate::target::Target;

const BLIND: &str = r#"{"modalities":{"vision":false}}"#;

/// A stored chat whose one message holds an image: its id.
async fn chat(ctx: &CliContext) -> i64 {
    let image = ctx.app.attachments().ingest(&png(64, 32)).await;
    let history = ctx.app.chat_history();
    let conversation = NewConversation {
        title: "Screenshots".to_owned(),
        ..NewConversation::default()
    };
    let id = history.create_conversation(conversation).await;
    let id = id.expect("saved");
    let message = NewMessage {
        conversation_id: id,
        role: MessageRole::User,
        content: "what is this?".to_owned(),
        metadata: None,
        images: vec![image.expect("stored").info.id],
    };
    history.save_message(message).await.expect("saved");
    id
}

/// Everything chat `id` stores: its row, and its messages.
async fn stored(ctx: &CliContext, id: i64) -> Value {
    let history = ctx.app.chat_history();
    let row = history.get_conversation(id).await.expect("read");
    let messages = history.get_messages(id).await.expect("read");
    json!({ "row": row, "messages": messages })
}

/// `gglib chat qwen --continue <id> --port <port>`.
fn resume(id: i64, port: u16) -> ChatArgs {
    ChatArgs {
        continue_id: Some(id),
        port: Some(port),
        no_tools: true,
        ..chat_args()
    }
}

/// A run as the daemon lists it.
fn listed(id: &str, status: RunStatus, conversation_id: Option<i64>) -> RunInfo {
    RunInfo {
        id: id.to_owned(),
        kind: RunKind::Agent,
        status,
        model: None,
        device: None,
        created_at_ms: 0,
        finished_at_ms: None,
        conversation_id,
        last_seq: 0,
        error: None,
        frames: gglib_core::domain::runs::RunFrames::Agent,
    }
}

/// A stand-in for this build's daemon, whose runs are `runs`.
fn daemon(runs: Vec<RunInfo>) -> (u16, Asked) {
    let listing = serde_json::to_string(&RunList { runs });
    answering(daemon_health(), listing.expect("a listing"))
}

/// The requests a stand-in was sent, by their first lines.
fn lines(asked: &Asked) -> Vec<String> {
    let asked = asked.lock().unwrap();
    asked.iter().map(|(line, _)| line.clone()).collect()
}

/// The two requests the question is: the probe, and the daemon's runs.
fn the_question() -> Vec<String> {
    [paths::HEALTH_PATH, paths::RUNS_PATH]
        .map(|path| format!("GET {path} HTTP/1.1"))
        .to_vec()
}

/// The daemon lists a run still going on the chat, after one on another
/// chat and one that has ended on this one. The chat is refused in a
/// sentence that names it and the run, and nothing a session stores on its
/// way in is stored: not the image `--image` names, not the Thinking choice
/// `--thinking` has a chat remember, not a row. Its server is asked nothing.
/// `--remote` changes none of it: the rows are this machine's either way.
#[tokio::test]
async fn a_chat_the_daemon_is_replying_to_is_refused_by_name_before_anything_is_stored() {
    gglib_core::paths::isolate_data_root();
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    let id = chat(&ctx).await;
    let before = stored(&ctx, id).await;
    let attached = png(8, 8);

    for target in [Target::Local, Target::Remote] {
        let server = props_server(BLIND);
        let args = ChatArgs {
            images: vec![file(&dir, "new.png", &attached)],
            thinking: Some(Thinking::Off),
            target,
            ..resume(id, server.port)
        };
        let (port, asked) = daemon(vec![
            listed("on-another", RunStatus::InProgress, Some(id + 1)),
            listed("ended", RunStatus::Completed, Some(id)),
            listed("chat-5b1e", RunStatus::InProgress, Some(id)),
        ]);

        let refused = STAND_IN_PORT.scope(port, run(&ctx, &args)).await;

        let refused = refused.expect_err("the daemon is replying to it");
        assert_eq!(
            refused.to_string(),
            format!(
                "chat {id} is running elsewhere: the daemon is still replying to it (run \
                 chat-5b1e). Wait for the reply, or stop it with: gglib run cancel chat-5b1e"
            ),
            "{target:?}"
        );
        assert_eq!(stored(&ctx, id).await, before, "{target:?}");
        let image = AttachmentId::of(&attached);
        assert!(ctx.app.attachments().info(&image).await.is_err());
        assert!(server.requests().is_empty(), "{target:?}");
        assert_eq!(lines(&asked), the_question(), "{target:?}");
    }
}

/// The chat continues as it did, here as far as its server's refusal: with
/// nothing on the daemon's port; with another program there; with a daemon
/// whose runs on the chat have ended; and with a daemon that does not answer
/// with its runs, as another data root's does not. Nothing is asked twice,
/// and only a daemon is asked for its runs.
#[tokio::test]
async fn a_chat_no_daemon_says_it_is_replying_to_continues() {
    gglib_core::paths::isolate_data_root();
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    let id = chat(&ctx).await;
    let another_program = r#"{"service":"something-else"}"#.to_owned();
    let free = daemon(vec![
        listed("on-another", RunStatus::InProgress, Some(id + 1)),
        listed("on-none", RunStatus::Queued, None),
        listed("ended", RunStatus::Completed, Some(id)),
        listed("stopped", RunStatus::Cancelled, Some(id)),
    ]);

    for (what, (port, asked), question) in [
        ("no daemon", (nobody(), Asked::default()), Vec::new()),
        (
            "another program",
            answering(another_program, "{}".to_owned()),
            the_question()[..1].to_vec(),
        ),
        ("a daemon with no live run on it", free, the_question()),
        (
            "a daemon that lists no runs",
            answering(daemon_health(), "{}".to_owned()),
            the_question(),
        ),
    ] {
        let server = props_server(BLIND);

        let went_on = STAND_IN_PORT
            .scope(port, run(&ctx, &resume(id, server.port)))
            .await;

        let went_on = went_on.expect_err("the server cannot see").to_string();
        assert!(
            went_on.starts_with("Model 'qwen' cannot read images"),
            "{what}: {went_on}"
        );
        assert_eq!(server.requests(), ["GET /props HTTP/1.1"], "{what}");
        assert_eq!(lines(&asked), question, "{what}");
    }
}
