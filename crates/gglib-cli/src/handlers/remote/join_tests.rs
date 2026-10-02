//! Tests for the command line of `gglib remote join`: the only name the
//! dial to another machine answers to.
//!
//! Parsed rather than run, because running `join` needs a daemon and a far
//! machine, and what these guard is the parser's half — and, from the
//! daemon's answer, what it prints.

use clap::Parser as _;
use clap::error::ErrorKind;
use gglib_app_services::RemoteJoinResponse;

use super::joined_lines;
use crate::commands::{Commands, RemoteCommand};
use crate::parser::Cli;

const PAIRING: &str = "pipeadlvvgabqkyqvn6vjp7nhslea45a5yls6pnkmizfv4bbu2hxa5iruaaauhlp2na-483920";

/// `connect` is not a subcommand, hidden or otherwise: clap refuses it as it
/// refuses any word it does not know, rather than running `join` under it.
#[test]
fn connect_is_not_a_subcommand_of_remote() {
    let Err(refused) = Cli::try_parse_from(["gglib", "remote", "connect", PAIRING]) else {
        panic!("`gglib remote connect` parsed");
    };
    assert_eq!(refused.kind(), ErrorKind::InvalidSubcommand, "{refused}");
}

/// `join` takes the pairing string and every flag it documents.
#[test]
fn join_takes_the_pairing_and_every_flag() {
    let cli = Cli::try_parse_from([
        "gglib",
        "remote",
        "join",
        PAIRING,
        "--port",
        "8181",
        "--relay",
        "https://relay.example",
        "--no-discovery",
    ])
    .expect("`gglib remote join` with every flag parses");
    let Some(Commands::Remote {
        command:
            RemoteCommand::Join {
                pairing,
                port,
                relay,
                no_discovery,
            },
    }) = cli.command
    else {
        panic!("expected `remote join`");
    };
    assert_eq!(pairing.as_deref(), Some(PAIRING));
    assert_eq!(port, Some(8181));
    assert_eq!(relay.as_deref(), Some("https://relay.example"));
    assert!(no_discovery);
}

/// With nothing after it, `join` dials the stored pairing, so every part is
/// optional.
#[test]
fn a_bare_join_parses_with_nothing_set() {
    let cli = Cli::try_parse_from(["gglib", "remote", "join"]).expect("a bare join parses");
    assert!(matches!(
        cli.command,
        Some(Commands::Remote {
            command: RemoteCommand::Join {
                pairing: None,
                port: None,
                relay: None,
                no_discovery: false,
            },
        })
    ));
}

/// The daemon's answer to a join of machine `e5a1b0c2d3f4`.
fn joined(paired: bool, replaced: Option<&str>) -> RemoteJoinResponse {
    RemoteJoinResponse {
        port: 8180,
        base_url: "http://127.0.0.1:8180/v1".to_owned(),
        ticket_fingerprint: "e5a1b0c2d3f4".to_owned(),
        paired,
        moved_from: None,
        replaced: replaced.map(str::to_owned),
    }
}

/// A pairing that replaced another machine's names that machine (#1042):
/// settings keep one pairing, and reaching the first machine again now takes
/// a fresh invite there. The whole output is pinned, so it is also the proof
/// that nothing else — a key above all — is printed beside it.
#[test]
fn a_pairing_over_another_machines_names_the_one_it_replaces() {
    assert_eq!(
        joined_lines(&joined(true, Some("d75a980182b1"))),
        [
            "  \u{2705} Paired with e5a1b0c2d3f4 and joined. Its API key is stored here; next \
             time the ticket alone, or nothing, will do.",
            "  This replaces the pairing with d75a980182b1: this machine keeps one pairing, so \
             reaching d75a980182b1 again takes a fresh `gglib remote invite` there.",
        ]
    );
}

/// With nothing replaced there is nothing more to say, paired or not.
#[test]
fn a_join_that_replaced_nothing_says_nothing_more() {
    assert_eq!(
        joined_lines(&joined(true, None)),
        [
            "  \u{2705} Paired with e5a1b0c2d3f4 and joined. Its API key is stored here; next \
             time the ticket alone, or nothing, will do."
        ]
    );
    assert_eq!(
        joined_lines(&joined(false, None)),
        ["  \u{2705} Joined e5a1b0c2d3f4."]
    );
}
