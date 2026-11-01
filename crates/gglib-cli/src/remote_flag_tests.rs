//! Tests for [`super`]: where `--remote` parses, and what it is told where
//! it does not.

use clap::CommandFactory as _;

use super::*;
use crate::commands::Commands;
use crate::target::REACHES;

fn remote(argv: &[&str]) -> bool {
    match parse_from(argv) {
        Ok((_, remote)) => remote,
        Err(refusal) => panic!("`{}` was refused: {refusal:?}", argv.join(" ")),
    }
}

fn refused(argv: &[&str]) -> String {
    match parse_from(argv) {
        Err(Refusal::Remote(sentence)) => sentence,
        Err(Refusal::Clap(error)) => panic!("`{}` got clap's error: {error}", argv.join(" ")),
        Ok(_) => panic!("`{}` parsed", argv.join(" ")),
    }
}

/// The flag parses after each command that reaches the paired machine, and
/// says so; without it the same command is for this machine.
#[test]
fn each_command_that_reaches_the_paired_machine_takes_the_flag() {
    for argv in [
        &["gglib", "chat", "--remote"][..],
        &["gglib", "q", "--remote", "hi"],
        &["gglib", "serve", "7", "--remote"],
        &["gglib", "model", "list", "--remote"],
        &["gglib", "model", "inspect", "7", "--remote"],
        &["gglib", "proxy", "dashboard", "--remote"],
        &["gglib", "proxy", "cache-clear", "--remote"],
        &["gglib", "daemon", "stop", "--remote"],
    ] {
        assert!(remote(argv), "{}", argv.join(" "));
        let without: Vec<&str> = argv.iter().copied().filter(|a| *a != "--remote").collect();
        assert!(!remote(&without), "{}", without.join(" "));
    }
}

/// A command that is about this machine refuses the flag with ADR 0013's
/// sentence, word for word, not with clap's "unexpected argument".
#[test]
fn a_command_about_this_machine_refuses_the_flag_with_the_one_sentence() {
    let text = refused(&["gglib", "image", "a cat", "--remote"]);
    assert_eq!(
        text,
        format!(
            "`gglib image` is about this machine, and --remote does not change that. \
             What --remote reaches on the paired machine: {REACHES}."
        )
    );
    let text = refused(&["gglib", "model", "remove", "3", "--remote"]);
    assert!(
        text.starts_with("`gglib model` is about this machine"),
        "{text}"
    );
}

/// The commands with a reason of their own keep it.
#[test]
fn a_command_with_its_own_reason_keeps_its_sentence() {
    let text = refused(&["gglib", "proxy", "stop", "--remote"]);
    assert!(text.starts_with("`gglib proxy stop` --remote:"), "{text}");
    assert!(text.contains("gglib daemon stop --remote"), "{text}");
    let text = refused(&["gglib", "web", "--remote"]);
    assert!(
        text.contains("already lists the paired machine's models"),
        "{text}"
    );
}

/// Before the command it belongs to, the flag is not ignored and not
/// refused as if the command were local: the line says where it goes.
#[test]
fn the_flag_before_its_command_is_told_where_it_goes() {
    let text = refused(&["gglib", "--remote", "chat"]);
    assert!(text.contains("goes after the command"), "{text}");
    assert!(text.contains("`gglib chat … --remote`"), "{text}");
    let text = refused(&["gglib", "proxy", "--remote", "dashboard"]);
    assert!(
        text.contains("`gglib proxy dashboard … --remote`"),
        "{text}"
    );
}

/// A line that names no command, or does not parse without the flag
/// either, gets clap's own error, which says what is actually wrong.
#[test]
fn a_line_with_nothing_to_refuse_gets_clap_s_error() {
    for argv in [&["gglib", "--remote"][..], &["gglib", "image", "--remote"]] {
        assert!(
            matches!(parse_from(argv), Err(Refusal::Clap(_))),
            "{}",
            argv.join(" ")
        );
    }
}

/// `--remote` and `--port` name different machines, so they exclude each
/// other on both commands that take both.
#[test]
fn remote_and_port_are_exclusive_on_both_agent_commands() {
    for argv in [
        &["gglib", "q", "--remote", "--port", "9000", "hi"][..],
        &["gglib", "chat", "--remote", "--port", "9000", "m"],
    ] {
        assert!(
            matches!(parse_from(argv), Err(Refusal::Clap(_))),
            "{}",
            argv.join(" ")
        );
    }
    match parse_from(["gglib", "q", "--remote", "hi"]) {
        Ok((cli, true)) => match cli.command {
            Some(Commands::Question { upstream, .. }) => assert_eq!(upstream.port, None),
            _ => panic!("expected the Question variant"),
        },
        _ => panic!("q --remote did not parse as remote"),
    }
}

/// The root's own help offers no `--remote`: there is nothing at the root
/// for it to do.
#[test]
fn the_root_does_not_offer_the_flag() {
    let root = Cli::command();
    assert!(root.get_arguments().all(|arg| arg.get_id() != REMOTE));
}
