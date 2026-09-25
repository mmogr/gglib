//! Tests for [`super`] — the decisions that depend on which machine a turn
//! runs on, without a machine.
//!
//! Everything that needs a daemon is below the seam and is exercised where
//! it always was; what is checked here is the table, the refusal, the two
//! pure decisions, and what remembering a model writes to a settings store
//! that another writer shares.

use std::sync::Mutex;

use async_trait::async_trait;
use clap::Parser as _;
use gglib_core::RemotePairing;
use gglib_core::ports::RepositoryError;

use super::*;
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
/// paired machine the positional is the wire name, because nothing here can
/// resolve it and `""` comes back as `404 Model '' not found`.
#[test]
fn the_wire_name_is_the_positional_only_on_the_paired_machine() {
    let flag = Some("typed".to_owned());
    assert_eq!(Target::Local.wire_model_name(flag.clone(), "qwen"), flag);
    assert_eq!(Target::Local.wire_model_name(None, "qwen"), None);
    assert_eq!(Target::Remote.wire_model_name(flag.clone(), "qwen"), flag);
    assert_eq!(
        Target::Remote.wire_model_name(None, "qwen"),
        Some("qwen".to_owned())
    );
    assert_eq!(Target::Remote.wire_model_name(None, ""), None);
}

#[test]
fn the_flag_is_the_only_way_to_the_paired_machine() {
    assert_eq!(Target::from_flag(false), Target::Local);
    assert_eq!(Target::from_flag(true), Target::Remote);
    assert_eq!(Target::default(), Target::Local);
}

// ── Remembering the model ────────────────────────────────────────────────

const TICKET_A: &str = "pipe-machine-a";
const TICKET_B: &str = "pipe-machine-b";

/// A write that lands between a turn's read of the settings and its write.
type Between = Box<dyn FnOnce(&mut Settings) + Send>;

/// A settings store another writer shares: the first `load` answers with
/// the settings as they stood, and then `between` lands, the way a write
/// from the daemon would while the turn is under way.
struct Shared {
    stored: Mutex<Settings>,
    between: Mutex<Option<Between>>,
}

impl Shared {
    fn paired_with_a(between: impl FnOnce(&mut Settings) + Send + 'static) -> Self {
        let mut settings = Settings::with_defaults();
        settings.remote_pairing = pairing(TICKET_A, "key-a");
        Self {
            stored: Mutex::new(settings),
            between: Mutex::new(Some(Box::new(between))),
        }
    }

    fn pairing(&self) -> Option<RemotePairing> {
        self.stored.lock().unwrap().remote_pairing.clone()
    }
}

#[async_trait]
impl SettingsRepository for Shared {
    async fn load(&self) -> Result<Settings, RepositoryError> {
        let mut stored = self.stored.lock().unwrap();
        let read = stored.clone();
        if let Some(write) = self.between.lock().unwrap().take() {
            write(&mut stored);
        }
        Ok(read)
    }

    async fn save(&self, settings: &Settings) -> Result<(), RepositoryError> {
        *self.stored.lock().unwrap() = settings.clone();
        Ok(())
    }
}

fn pairing(ticket: &str, api_key: &str) -> Option<RemotePairing> {
    Some(RemotePairing {
        ticket: ticket.to_owned(),
        api_key: api_key.to_owned(),
        default_model: None,
        port: Some(8180),
    })
}

/// A model named on a turn is remembered on the pairing it was named for.
#[tokio::test]
async fn a_model_named_on_a_turn_is_remembered_on_its_pairing() {
    let store = Shared::paired_with_a(|_| {});

    let model = remembered_model(&store, "qwen3".to_owned()).await;

    assert_eq!(model.expect("a model was named"), "qwen3");
    let stored = store.pairing().expect("the pairing is still stored");
    assert_eq!(stored.default_model.as_deref(), Some("qwen3"));
    assert_eq!(stored.api_key, "key-a", "remembering touched the key");
}

/// Remembering a model keeps the key a re-pair wrote while the turn ran.
#[tokio::test]
async fn remembering_a_model_keeps_the_key_a_re_pair_wrote() {
    let store = Shared::paired_with_a(|now| now.remote_pairing = pairing(TICKET_A, "key-a-again"));

    remembered_model(&store, "qwen3".to_owned())
        .await
        .expect("a model was named");

    let stored = store.pairing().expect("the pairing is still stored");
    assert_eq!(
        stored.api_key, "key-a-again",
        "the re-pair's key was put back"
    );
    assert_eq!(stored.default_model.as_deref(), Some("qwen3"));
}

/// A model named for one machine is not remembered on a pairing with
/// another, which a join wrote while the turn ran.
#[tokio::test]
async fn a_model_is_not_remembered_for_another_machine() {
    let store = Shared::paired_with_a(|now| now.remote_pairing = pairing(TICKET_B, "key-b"));

    remembered_model(&store, "qwen3".to_owned())
        .await
        .expect("the turn still runs on the model it named");

    assert_eq!(
        store.pairing(),
        pairing(TICKET_B, "key-b"),
        "the other machine's pairing was changed"
    );
}

/// A pairing forgotten while the turn ran stays forgotten.
#[tokio::test]
async fn a_forgotten_pairing_is_not_brought_back() {
    let store = Shared::paired_with_a(|now| now.remote_pairing = None);

    remembered_model(&store, "qwen3".to_owned())
        .await
        .expect("the turn still runs on the model it named");

    assert_eq!(store.pairing(), None, "remembering a model brought it back");
}
