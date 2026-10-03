//! Tests for [`super`] — the decisions that depend on which machine a turn
//! runs on, without a machine.
//!
//! Everything that needs a daemon is below the seam and is exercised where
//! it always was; what is checked here is the table, the refusal and the two
//! pure decisions. What a turn's model resolves to, and what remembering it
//! writes, is `target_turn_tests`'.

use clap::Parser as _;

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
