//! Which machine a resumed conversation runs on: the one its stored model
//! is on, whatever the flag says; the flag's, when it stored none or a model
//! is named on the command line. And what a resume on another model saves.

use gglib_core::Settings;
use gglib_core::domain::ModelRef;

use super::super::super::{Session, prepare};
use super::*;
use crate::bootstrap::{CliContext, test_context};

const DESK: &str = "0123456789ab";

/// A stored conversation with these `settings`, and no `model_id`.
fn row(settings: Option<ConversationSettings>) -> Conversation {
    Conversation {
        id: 1,
        title: "t".to_owned(),
        model_id: None,
        system_prompt: None,
        settings,
        created_at: String::new(),
        updated_at: String::new(),
    }
}

/// A conversation that stored `machine`'s model 3, shown as `qwen3`, with
/// `profile`.
fn saved(machine: Machine, profile: Option<&str>) -> Conversation {
    row(Some(ConversationSettings {
        model_name: Some("qwen3".to_owned()),
        model: Some(ModelRef { machine, id: 3 }),
        profile: profile.map(str::to_owned),
        ..ConversationSettings::default()
    }))
}

fn desk() -> Machine {
    Machine::Paired {
        fingerprint: DESK.to_owned(),
    }
}

/// This machine is paired with desk.
fn paired_with_desk(fingerprint: &str) -> bool {
    fingerprint == DESK
}

/// A chat that ran on desk resumes on desk without `--remote`, by its id
/// there and the profile it was routed to: what goes on the wire.
#[test]
fn a_far_chat_resumes_on_its_machine_without_the_flag() {
    let plain = stored_machine(
        &saved(desk(), None),
        Target::Local,
        false,
        false,
        paired_with_desk,
    );
    let profiled = stored_machine(
        &saved(desk(), Some("coding")),
        Target::Remote,
        false,
        false,
        paired_with_desk,
    );

    assert_eq!(plain.unwrap(), Some((Target::Remote, "3".to_owned())));
    assert_eq!(
        profiled.unwrap(),
        Some((Target::Remote, "3:coding".to_owned()))
    );
}

/// After re-pairing with another machine, a chat that ran on desk is
/// refused rather than sent to a model 3 that is some other machine's.
#[test]
fn a_far_chat_is_refused_once_paired_with_another_machine() {
    let refused = stored_machine(&saved(desk(), None), Target::Remote, false, false, |_| {
        false
    })
    .expect_err("refused");

    let text = refused.to_string();
    assert!(text.contains("no longer paired with"), "{text}");
    assert!(!text.contains(DESK), "no fingerprint: {text}");
}

/// A chat that ran on desk is not sent to a server here by `--port`, nor
/// sent to desk in spite of it: the two name different machines.
#[test]
fn a_far_chat_refuses_port() {
    let refused = stored_machine(
        &saved(desk(), None),
        Target::Local,
        true,
        false,
        paired_with_desk,
    )
    .expect_err("refused");

    let text = refused.to_string();
    assert!(text.contains("drop --port"), "{text}");
    assert!(!text.contains(DESK), "no fingerprint: {text}");
}

/// A chat that ran here is not sent to the paired machine by `--remote`.
#[test]
fn a_local_chat_refuses_remote() {
    let refused = stored_machine(
        &saved(Machine::Local, None),
        Target::Remote,
        false,
        false,
        paired_with_desk,
    )
    .expect_err("refused");

    assert!(refused.to_string().contains("drop --remote"), "{refused}");
}

/// A chat that ran here resumes here by its id, not by the name it was
/// shown by, which a model on the paired machine may share.
#[test]
fn a_local_chat_resumes_here_by_its_id() {
    let resumed = stored_machine(
        &saved(Machine::Local, Some("coding")),
        Target::Local,
        false,
        false,
        paired_with_desk,
    );

    assert_eq!(resumed.unwrap(), Some((Target::Local, "3".to_owned())));
}

/// A row that stored no model, and a model named on the command line, each
/// leave the machine to the flag.
#[test]
fn without_a_stored_model_the_flag_decides() {
    let older = row(Some(ConversationSettings {
        model_name: Some("qwen3".to_owned()),
        ..ConversationSettings::default()
    }));
    for flag in [Target::Local, Target::Remote] {
        assert_eq!(
            stored_machine(&older, flag, false, false, paired_with_desk).unwrap(),
            None
        );
        assert_eq!(
            stored_machine(&row(None), flag, false, false, paired_with_desk).unwrap(),
            None
        );
        assert_eq!(
            stored_machine(&saved(desk(), None), flag, false, true, |_| false).unwrap(),
            None,
            "a named model follows the flag, and is not refused"
        );
    }
}

/// Following the stored machine rewrites the resume's target and
/// identifier, from a conversation that stored desk's model 3.
#[test]
fn following_the_stored_machine_sets_the_target_and_the_identifier() {
    let mut args = ChatArgs {
        identifier: String::new(),
        target: Target::Local,
        ..super::super::tests::chat_args()
    };

    follow_stored_machine(&mut args, &saved(Machine::Local, None), None)
        .expect("a local chat resumes here");

    assert_eq!(
        (args.target, args.identifier.as_str()),
        (Target::Local, "3")
    );
}

// ── What a resume saves ──────────────────────────────────────────────────

/// The CLI's context over `dir`'s database, paired with desk, with `qwen`
/// and `llama` in its catalogue: their ids.
async fn library(dir: &tempfile::TempDir) -> (CliContext, i64, i64) {
    let ctx = test_context(dir.path()).await;
    ctx.settings_repo
        .modify(&|settings: &mut Settings| {
            settings.remote_pairing = Some(gglib_core::RemotePairing {
                ticket: "not-a-ticket".to_owned(),
                api_key: "key".to_owned(),
                default_model: None,
                port: None,
                name: Some("desk".to_owned()),
            });
            Ok(())
        })
        .await
        .expect("paired");
    let mut ids = Vec::new();
    for name in ["qwen", "llama"] {
        let model = gglib_core::domain::NewModel::new(
            name.to_owned(),
            dir.path().join(format!("{name}.gguf")),
            8.0,
            chrono::Utc::now(),
        );
        ids.push(ctx.app.models().add(model).await.expect("registered").id);
    }
    (ctx, ids[0], ids[1])
}

/// `gglib chat [<identifier>] [--continue <id>]`, prepared.
async fn chat<'a>(ctx: &'a CliContext, identifier: &str, id: Option<i64>) -> Result<Session<'a>> {
    let args = ChatArgs {
        identifier: identifier.to_owned(),
        continue_id: id,
        ..super::super::tests::chat_args()
    };
    prepare(ctx, &args).await
}

/// A chat started on qwen and continued with llama named stores llama, by
/// its id here, in the row's id too: a plain `--continue` then resumes on
/// llama, and the chat's id, model and name all say the same model.
#[tokio::test]
async fn a_resume_on_another_model_stores_that_model() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (ctx, _, llama) = library(&dir).await;
    let started = chat(&ctx, "qwen", None).await.expect("a new chat");
    let id = started.persistence.expect("saved").id;

    chat(&ctx, "llama", Some(id))
        .await
        .expect("continued on llama");

    let row = ctx.app.chat_history().get_conversation(id).await;
    let row = row.unwrap().unwrap();
    let settings = row.settings.expect("settings");
    assert_eq!(
        (row.model_id, settings.model, settings.model_name.as_deref()),
        (
            Some(llama),
            Some(ModelRef {
                machine: Machine::Local,
                id: llama
            }),
            Some("llama")
        )
    );
    let resumed = chat(&ctx, "", Some(id)).await.expect("resumed");
    assert_eq!(resumed.params.model_identifier, llama.to_string());
}

/// A chat the page or a phone switched thinking off on stays switched off
/// through a resume that moves it to another model: the resume rewrites the
/// settings whole, and keeps what it does not set. (The CLI's own turns do
/// not apply the choice.)
#[tokio::test]
async fn a_remembered_thinking_choice_survives_a_resume_on_another_model() {
    use gglib_core::domain::Thinking;

    let dir = tempfile::tempdir().expect("tempdir");
    let (ctx, _, _) = library(&dir).await;
    let started = chat(&ctx, "qwen", None).await.expect("a new chat");
    let id = started.persistence.expect("saved").id;
    let history = ctx.app.chat_history();
    let off = Some(Thinking::Off);
    history.record_thinking(id, off).await.expect("remembered");

    chat(&ctx, "llama", Some(id))
        .await
        .expect("continued on llama");

    let row = history.get_conversation(id).await.unwrap().unwrap();
    let settings = row.settings.expect("settings");
    assert_eq!(
        (settings.model_name.as_deref(), settings.thinking),
        (Some("llama"), off)
    );
}

/// A chat whose model has left this library says so, and does not send the
/// user to the paired machine, which `--remote` would refuse for this chat.
#[tokio::test]
async fn a_chat_whose_model_is_gone_says_so_and_not_to_use_remote() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (ctx, qwen, _) = library(&dir).await;
    let started = chat(&ctx, "qwen", None).await.expect("a new chat");
    let id = started.persistence.expect("saved").id;
    ctx.app.models().delete(qwen).await.expect("removed");

    let refused = chat(&ctx, "", Some(id)).await.err().expect("refused");

    let text = refused.to_string();
    assert!(text.contains("no longer in this library"), "{text}");
    assert!(text.contains(&format!("--continue {id}")), "{text}");
    assert!(!text.contains("--remote"), "{text}");
}
