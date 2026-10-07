//! What a chat stores of its model when it is made, and what a resume reads
//! from a stored chat in each shape one has been saved in: made today; saved
//! before a chat's `model_id` followed its settings; saved before a chat
//! stored its model at all; and saved with its system prompt as a message.
//!
//! Each test runs `prepare` over a database of its own. A row in an older
//! shape that the service no longer writes is written past it, as the store
//! keeps it.

use gglib_core::domain::agent::AgentMessage;
use gglib_core::domain::chat::{MessageRole, NewConversation, NewMessage};
use gglib_core::domain::{Machine, ModelRef};

use super::super::prepare;
use super::tests::chat_args;
use super::*;
use crate::bootstrap::{CliContext, test_context};

/// A model of this machine's catalogue named `name`: its id.
async fn registered(ctx: &CliContext, dir: &tempfile::TempDir, name: &str) -> i64 {
    let path = dir.path().join(format!("{name}.gguf"));
    let model = gglib_core::domain::NewModel::new(name.to_owned(), path, 8.0, chrono::Utc::now());
    ctx.app.models().add(model).await.expect("registered").id
}

/// A row as the store keeps it, written past the service: through the
/// repository of a second opening of `dir`'s database.
async fn stored_as_is(dir: &tempfile::TempDir, conv: NewConversation) -> i64 {
    let config = gglib_bootstrap::BootstrapConfig {
        db_path: dir.path().join("gglib.db"),
        models_dir: dir.path().join("models"),
    };
    let emitter = std::sync::Arc::new(gglib_core::ports::NoopEmitter::new());
    let built = gglib_bootstrap::CoreBootstrap::build(config, emitter).await;
    let store = built.expect("the database opens").repos.chat_history;
    store
        .create_conversation(conv)
        .await
        .expect("the row is saved")
}

async fn row(ctx: &CliContext, id: i64) -> gglib_core::domain::chat::Conversation {
    let read = ctx.app.chat_history().get_conversation(id).await;
    read.expect("read").expect("the conversation is there")
}

/// `gglib chat --continue <id>`, with no model named.
fn resume(id: i64) -> ChatArgs {
    ChatArgs {
        identifier: String::new(),
        continue_id: Some(id),
        ..chat_args()
    }
}

/// Settings that name `qwen` and no model, as rows from before a chat stored
/// its model do.
fn named() -> ConversationSettings {
    ConversationSettings {
        model_name: Some("qwen".to_owned()),
        ..ConversationSettings::default()
    }
}

/// `gglib chat qwen` on a model this catalogue holds saves it as the row's
/// `model_id` too, which is where a paired phone's chat list reads it.
#[tokio::test]
async fn a_chat_started_on_this_machines_model_stores_its_id() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    let qwen = registered(&ctx, &dir, "qwen").await;

    let started = prepare(&ctx, &chat_args()).await.expect("a new chat");

    let made = row(&ctx, started.persistence.expect("saved").id).await;
    let here = ModelRef {
        machine: Machine::Local,
        id: qwen,
    };
    assert_eq!(made.model_id, Some(qwen));
    assert_eq!(made.settings.and_then(|s| s.model), Some(here));
}

/// `gglib chat` and `gglib q` both save their session through
/// `Conversation::create`. A model of this machine's in its settings is the
/// row's `model_id`; the paired machine's is not, though it has the number
/// of a model here.
#[tokio::test]
async fn a_saved_session_stores_the_id_of_this_machines_model_and_not_the_paired_ones() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    let qwen = registered(&ctx, &dir, "qwen").await;
    let fingerprint = "0123456789ab".to_owned();

    for (machine, stored) in [
        (Machine::Local, Some(qwen)),
        (Machine::Paired { fingerprint }, None),
    ] {
        let turn = TurnModel {
            model_ref: Some(ModelRef { machine, id: qwen }),
            ..TurnModel::here("qwen".to_owned(), None)
        };
        let settings = session_settings(&chat_args(), None, &turn);
        let (chats, made_by) = (ctx.app.chat_history(), turn.made_by());
        let saved = Conversation::create(chats, None, Some(settings.clone()), made_by);
        let made = row(&ctx, saved.await.expect("saved").id).await;
        assert_eq!((made.model_id, made.settings), (stored, Some(settings)));
    }
}

/// A resume on the model a chat stores records nothing: the row is as it
/// was. One saved before a chat's `model_id` followed its settings keeps the
/// empty id it has, and resumes on its model all the same.
#[tokio::test]
async fn a_resume_on_the_chats_own_model_leaves_its_row_as_it_was() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    let qwen = registered(&ctx, &dir, "qwen").await;
    let started = prepare(&ctx, &chat_args()).await.expect("a new chat");
    let today = started.persistence.expect("saved").id;
    let older = NewConversation {
        title: "older".to_owned(),
        settings: row(&ctx, today).await.settings,
        ..NewConversation::default()
    };
    let older = stored_as_is(&dir, older).await;

    for (id, model_id) in [(today, Some(qwen)), (older, None)] {
        let before = row(&ctx, id).await;
        assert_eq!(before.model_id, model_id);

        let session = prepare(&ctx, &resume(id)).await.expect("resumed");

        assert_eq!(session.params.model_identifier, qwen.to_string());
        let after = row(&ctx, id).await;
        assert_eq!(
            (after.model_id, after.settings, after.updated_at),
            (model_id, before.settings, before.updated_at)
        );
    }
}

/// A chat from before a conversation stored its model holds only this
/// machine's `model_id`. It ran here, as the daemon reads it: `--remote` is
/// refused, and without it the chat resumes here by the name it saved.
#[tokio::test]
async fn a_chat_that_stores_only_a_model_id_resumes_here_and_refuses_remote() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    let qwen = registered(&ctx, &dir, "qwen").await;
    let older = NewConversation {
        title: "older".to_owned(),
        model_id: Some(qwen),
        settings: Some(named()),
        ..NewConversation::default()
    };
    let id = ctx.app.chat_history().create_conversation(older).await;
    let id = id.expect("saved");
    assert_eq!(row(&ctx, id).await.model_id, Some(qwen));

    let remote = ChatArgs {
        target: Target::Remote,
        ..resume(id)
    };
    let refused = prepare(&ctx, &remote).await.err().expect("refused");
    let here = prepare(&ctx, &resume(id)).await.expect("resumed here");

    assert!(refused.to_string().contains("drop --remote"), "{refused}");
    assert_eq!(
        (here.params.target, here.params.model_identifier.as_str()),
        (Target::Local, "qwen")
    );
}

/// A chat with `prompt` as its system prompt and three rows: `OLD-PROMPT`
/// as a system row when `system_row`, then `first` and its `answer`.
async fn chat(ctx: &CliContext, prompt: Option<&str>, system_row: bool) -> i64 {
    let history = ctx.app.chat_history();
    let conv = NewConversation {
        title: "t".to_owned(),
        system_prompt: prompt.map(str::to_owned),
        settings: Some(named()),
        ..NewConversation::default()
    };
    let id = history.create_conversation(conv).await.expect("saved");
    let rows = [
        (MessageRole::System, "OLD-PROMPT"),
        (MessageRole::User, "first"),
        (MessageRole::Assistant, "answer"),
    ];
    for (role, content) in rows.into_iter().skip(usize::from(!system_row)) {
        let row = NewMessage {
            conversation_id: id,
            role,
            content: content.to_owned(),
            metadata: None,
            images: Vec::new(),
        };
        history.save_message(row).await.expect("saved");
    }
    id
}

/// A message as its role and what it says.
fn said(message: &AgentMessage) -> (&'static str, &str) {
    match message {
        AgentMessage::System { content } => ("system", content),
        AgentMessage::User { content, .. } => ("user", content),
        AgentMessage::Assistant { content } => ("assistant", content.text.as_deref().unwrap_or("")),
        AgentMessage::Tool { content, .. } => ("tool", content),
    }
}

/// What a resume of `id` starts from, with `--system-prompt` when `flag`.
async fn resumed(ctx: &CliContext, id: i64, flag: Option<&str>) -> Vec<(&'static str, String)> {
    let args = ChatArgs {
        system_prompt: flag.map(str::to_owned),
        ..resume(id)
    };
    let session = prepare(ctx, &args).await.expect("resumed");
    let history = session.prior_messages.iter().map(said);
    history
        .map(|(role, text)| (role, text.to_owned()))
        .collect()
}

/// A resumed chat starts from the history a paired device's turn starts
/// from: the conversation's prompt, trimmed, or the one this command line
/// names in its place, then every row but a system one. A chat saved with
/// its prompt as a message does not send it twice, and a blank prompt sends
/// no system message.
#[tokio::test]
async fn a_resume_reads_the_history_as_the_daemon_reads_it() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    let turns = [
        ("user", "first".to_owned()),
        ("assistant", "answer".to_owned()),
    ];
    let led_by = |prompt: &str| [vec![("system", prompt.to_owned())], turns.to_vec()].concat();

    let prompted = chat(&ctx, Some("  Be brief.\n"), true).await;
    assert_eq!(resumed(&ctx, prompted, None).await, led_by("Be brief."));
    assert_eq!(
        resumed(&ctx, prompted, Some(" Be long. ")).await,
        led_by("Be long.")
    );
    for blank in [None, Some("  \n")] {
        let id = chat(&ctx, blank, true).await;
        assert_eq!(resumed(&ctx, id, None).await, turns, "{blank:?}");
    }
}
