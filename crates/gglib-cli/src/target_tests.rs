//! Tests for [`super`] — the decisions that depend on which machine a turn
//! runs on, without a machine.
//!
//! Everything that needs a daemon is below the seam and is exercised where
//! it always was; what is checked here is the table, the refusal, the two
//! pure decisions, and what this machine's catalogue answers when it holds
//! no such model and when it cannot be read. What a turn's model resolves to
//! on the paired machine, and what remembering it writes, is
//! `target_turn_tests`'.

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use clap::Parser as _;
use gglib_core::domain::NewModel;
use gglib_core::ports::{ModelRepository, RepositoryError};
use gglib_core::services::AppCore;

use super::*;
use crate::bootstrap::test_context;
use crate::parser::Cli;

fn parsed(argv: &[&str]) -> Commands {
    Cli::parse_from(argv).command.expect("a subcommand")
}

// ── The table ────────────────────────────────────────────────────────────

/// The use side, and the line it does not cross: a turn, loading a model,
/// the catalogue, the dashboard, the cache, stopping the machine — and
/// nothing that changes what is on it.
#[test]
fn the_use_side_reaches_the_machine_and_everything_else_is_about_this_one() {
    for argv in [
        &["gglib", "chat"][..],
        &["gglib", "q", "hi"],
        &["gglib", "serve", "qwen3"],
        &["gglib", "model", "list"],
        &["gglib", "model", "inspect", "3"],
        &["gglib", "proxy", "dashboard"],
        &["gglib", "proxy", "cache-clear"],
        &["gglib", "daemon", "stop"],
    ] {
        let (name, reach) = reach(&parsed(argv));
        assert_eq!(reach, Reach::Use, "{name}");
    }
    for argv in [
        &["gglib", "model", "remove", "qwen3"][..],
        &["gglib", "remote", "status"],
        &["gglib", "proxy"],
        &["gglib", "daemon", "status"],
        &["gglib", "config", "settings", "show"],
        &["gglib", "completions", "bash"],
    ] {
        let (name, reach) = reach(&parsed(argv));
        assert_eq!(reach, Reach::Local, "{name}");
        assert_eq!(name, argv[1], "the name is the word typed");
    }
    // The loop guard's log is this machine's database; --remote must refuse
    // it rather than print this machine's log as if it were the far one's.
    assert_eq!(
        reach(&parsed(&["gglib", "proxy", "trips"])),
        ("proxy trips", Reach::Local)
    );
}

/// A turn, a load and every `model` subcommand reach the paired machine
/// exactly when core's table lets that action be done there: the CLI keeps
/// no list of its own.
#[test]
fn what_a_model_command_reaches_is_cores_table() {
    use gglib_core::domain::ModelAction;
    for (argv, action) in [
        (&["gglib", "chat", "3"][..], ModelAction::Chat),
        (&["gglib", "q", "hi"], ModelAction::Chat),
        (&["gglib", "serve", "3"], ModelAction::Load),
        (&["gglib", "model", "list"], ModelAction::List),
        (&["gglib", "model", "inspect", "3"], ModelAction::Detail),
        (&["gglib", "model", "remove", "3"], ModelAction::Manage),
        (&["gglib", "model", "retag", "3"], ModelAction::Manage),
        (&["gglib", "model", "explain", "3"], ModelAction::Manage),
        (&["gglib", "model", "add", "x.gguf"], ModelAction::Manage),
    ] {
        let (name, reach) = reach(&parsed(argv));
        let want = if action.on_paired() {
            Reach::Use
        } else {
            Reach::Local
        };
        assert_eq!(reach, want, "{name}: {action:?}");
    }
}

/// `model inspect --remote` reads one of the paired machine's models;
/// `model remove --remote` would change that machine and is refused with
/// the sentence that lists what `--remote` reaches, inspect among them.
#[test]
fn inspect_reaches_the_paired_machine_and_remove_does_not() {
    assert!(
        Target::Remote
            .admit(&parsed(&["gglib", "model", "inspect", "3"]))
            .is_ok()
    );
    let text = Target::Remote
        .admit(&parsed(&["gglib", "model", "remove", "3"]))
        .expect_err("refused")
        .to_string();
    assert!(text.contains(REACHES), "{text}");
    assert!(text.contains("model inspect"), "{text}");
}

/// `proxy stop` is about this machine for a reason the general sentence
/// would get wrong, so it gets its own — and it names what to run instead.
#[test]
fn stopping_the_far_proxy_is_refused_with_the_command_that_stops_the_machine() {
    let err = Target::Remote
        .admit(&parsed(&["gglib", "proxy", "stop"]))
        .expect_err("refused");
    let text = err.to_string();
    assert!(text.starts_with("`gglib proxy stop` --remote:"), "{text}");
    assert!(text.contains("gglib daemon stop --remote"), "{text}");
}

/// `web` and `gui` are refused because the page already shows the paired
/// machine, not because the page is about this machine alone.
#[test]
fn the_page_refuses_remote_because_it_already_shows_the_paired_machine() {
    for command in ["web", "gui"] {
        let err = Target::Remote
            .admit(&parsed(&["gglib", command]))
            .expect_err("refused");
        let text = err.to_string();
        assert!(
            text.starts_with(&format!("`gglib {command}` --remote:")),
            "{text}"
        );
        assert!(
            text.contains("already lists the paired machine's models"),
            "{text}"
        );
        assert!(!text.contains("is about this machine"), "{text}");
    }
}

/// The refusal is one sentence and it names both halves: the command that
/// stays local, and what `--remote` does reach.
#[test]
fn a_local_command_refuses_remote_with_the_one_sentence() {
    let err = Target::Remote
        .admit(&parsed(&["gglib", "model", "remove", "qwen3"]))
        .expect_err("refused");
    let text = err.to_string();
    assert!(
        text.starts_with("`gglib model` is about this machine"),
        "{text}"
    );
    assert!(text.contains("chat, q"), "{text}");
    assert!(
        Target::Remote.admit(&parsed(&["gglib", "chat"])).is_ok(),
        "a command that uses a machine is admitted"
    );
    assert!(
        Target::Local
            .admit(&parsed(&["gglib", "model", "list"]))
            .is_ok(),
        "without --remote nothing is refused"
    );
}

// ── The wire name ────────────────────────────────────────────────────────

/// Locally an absent `--model` leaves the wire name empty on purpose; on the
/// paired machine the positional, as that machine resolved it, is the wire
/// name, because `""` comes back as `404 Model '' not found`.
#[test]
fn the_wire_name_is_the_positional_only_on_the_paired_machine() {
    let flag = Some("typed".to_owned());
    assert_eq!(Target::Local.wire_model_name(flag.clone(), "qwen"), flag);
    assert_eq!(Target::Local.wire_model_name(None, "qwen"), None);
    assert_eq!(Target::Remote.wire_model_name(flag.clone(), "3"), flag);
    assert_eq!(
        Target::Remote.wire_model_name(None, "3:coding"),
        Some("3:coding".to_owned())
    );
    assert_eq!(Target::Remote.wire_model_name(None, ""), None);
}

#[test]
fn the_flag_is_the_only_way_to_the_paired_machine() {
    assert_eq!(Target::from_flag(false), Target::Local);
    assert_eq!(Target::from_flag(true), Target::Remote);
    assert_eq!(Target::default(), Target::Local);
}

// ── This machine's catalogue ─────────────────────────────────────────────

/// What a catalogue that cannot be read says.
pub(crate) const UNREADABLE: &str = "Storage error: database is locked";

/// A catalogue no call to which succeeds, as a locked database's does not.
struct Unreadable;

fn locked<T>() -> Result<T, RepositoryError> {
    Err(RepositoryError::Storage("database is locked".to_owned()))
}

#[async_trait]
impl ModelRepository for Unreadable {
    async fn list(&self) -> Result<Vec<Model>, RepositoryError> {
        locked()
    }
    async fn get_by_id(&self, _id: i64) -> Result<Model, RepositoryError> {
        locked()
    }
    async fn get_by_name(&self, _name: &str) -> Result<Model, RepositoryError> {
        locked()
    }
    async fn insert(&self, _model: &NewModel) -> Result<Model, RepositoryError> {
        locked()
    }
    async fn find_by_path(&self, _path: &Path) -> Result<Option<Model>, RepositoryError> {
        locked()
    }
    async fn update(&self, _model: &Model) -> Result<(), RepositoryError> {
        locked()
    }
    async fn delete(&self, _id: i64) -> Result<(), RepositoryError> {
        locked()
    }
}

/// The CLI's context in `dir`, over a catalogue that cannot be read. Its
/// other stores are an empty database's.
pub(crate) async fn with_unreadable_catalogue(dir: &tempfile::TempDir) -> CliContext {
    let mut ctx = test_context(dir.path()).await;
    let pool = gglib_db::setup_test_database().await.expect("a database");
    let mut repos = gglib_db::CoreFactory::build_repos(pool);
    repos.models = Arc::new(Unreadable);
    let (hf, downloads) = (Arc::clone(&ctx.hf_client), Arc::clone(&ctx.downloads));
    ctx.app = Arc::new(AppCore::new(repos, hf, downloads));
    ctx
}

/// A catalogue that cannot be read is an error wherever a turn looks its
/// model up, by id or by name, and the error is the store's own: it is not
/// taken for a model this machine does not have.
#[tokio::test]
async fn a_catalogue_that_cannot_be_read_is_an_error_and_not_a_missing_model() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = with_unreadable_catalogue(&dir).await;

    for identifier in ["qwen3", "7"] {
        let looked_up = Target::Local.local_model(&ctx, identifier).await;
        let error = looked_up.expect_err("the read failed").to_string();
        assert_eq!(error, UNREADABLE, "{identifier}");

        let turn = Target::Local.resolve_turn(&ctx, identifier.to_owned());
        let error = turn.await.expect_err("the read failed").to_string();
        assert_eq!(error, UNREADABLE, "{identifier}");
    }
    // The paired machine's models are not in this catalogue: it is not read.
    let far = Target::Remote.local_model(&ctx, "qwen3").await;
    assert!(far.expect("nothing was read").is_none());
}

/// A model this catalogue does not hold is no error: a session on `--port`
/// may name one, and its turn goes on under the name as typed.
#[tokio::test]
async fn a_model_this_catalogue_does_not_hold_is_none() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;

    let looked_up = Target::Local.local_model(&ctx, "qwen3").await;
    assert!(looked_up.expect("the catalogue was read").is_none());

    let turn = Target::Local.resolve_turn(&ctx, "qwen3".to_owned()).await;
    let turn = turn.expect("a turn on a model the catalogue lacks");
    assert_eq!((turn.name.as_str(), turn.model_ref), ("qwen3", None));
}
