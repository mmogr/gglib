//! Tests for [`super`] — the decisions that depend on which machine a turn
//! runs on, without a machine.
//!
//! Everything that needs a daemon or a settings store is below the seam and
//! is exercised where it always was; what is checked here is the table, the
//! refusal, and the two pure decisions.

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
