//! Tests for the command line of `gglib remote join`: the only name the
//! dial to another machine answers to.
//!
//! Parsed rather than run, because running `join` needs a daemon and a far
//! machine, and what these guard is the parser's half — and, from the
//! daemon's answer, what it prints.

use clap::Parser as _;
use clap::error::ErrorKind;
use gglib_app_services::{RemoteConnection, RemoteJoinResponse, RemoteStatus};
use gglib_core::domain::UNNAMED_PAIRED;

use super::{connection_line, joined_lines};
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
fn joined(paired: bool, name: Option<&str>, replaced: Option<&str>) -> RemoteJoinResponse {
    RemoteJoinResponse {
        port: 8180,
        base_url: "http://127.0.0.1:8180/v1".to_owned(),
        ticket_fingerprint: FINGERPRINT.to_owned(),
        name: name.map(str::to_owned),
        paired,
        moved_from: None,
        replaced: replaced.map(str::to_owned),
    }
}

/// The fingerprint of the machine joined: in the answer, as its identity, and
/// never on screen.
const FINGERPRINT: &str = "e5a1b0c2d3f4";

/// A pairing that replaced another machine's names both machines (#1042), by
/// name: settings keep one pairing, and reaching the first machine again now
/// takes a fresh invite there. The whole output is pinned, so it is also the
/// proof that nothing else — a key or a fingerprint above all — is printed.
#[test]
fn a_pairing_over_another_machines_names_the_one_it_replaces() {
    assert_eq!(
        joined_lines(&joined(true, Some("desk"), Some("laptop"))),
        [
            "  \u{2705} Joined desk, and paired with it. Its API key is stored here; next time \
             the ticket alone, or nothing, will do.",
            "  This replaces the pairing with laptop: this machine keeps one pairing, so \
             reaching it again takes a fresh `gglib remote invite` there.",
        ]
    );
}

/// When neither machine gave a name, the one replaced is not "the paired
/// machine" too: the two sentences would name the new machine and the one
/// dropped in the same words.
#[test]
fn an_unnamed_machine_replaced_by_an_unnamed_one_is_told_apart() {
    assert_eq!(
        joined_lines(&joined(true, None, Some(UNNAMED_PAIRED))),
        [
            "  \u{2705} Joined the paired machine, and paired with it. Its API key is stored \
             here; next time the ticket alone, or nothing, will do.",
            "  This replaces the pairing with the machine paired before: this machine keeps one \
             pairing, so reaching it again takes a fresh `gglib remote invite` there.",
        ]
    );
}

/// With nothing replaced there is nothing more to say, paired or not; a
/// machine that gave no name is shown in words, not by its fingerprint.
#[test]
fn a_join_that_replaced_nothing_says_nothing_more() {
    assert_eq!(
        joined_lines(&joined(true, Some("desk"), None)),
        [
            "  \u{2705} Joined desk, and paired with it. Its API key is stored here; next time \
             the ticket alone, or nothing, will do."
        ]
    );
    assert_eq!(
        joined_lines(&joined(false, Some("desk"), None)),
        ["  \u{2705} Joined desk."]
    );
    assert_eq!(
        joined_lines(&joined(false, None, None)),
        ["  \u{2705} Joined the paired machine."]
    );
}

/// `gglib remote status`'s connect-side line names the paired machine, up,
/// away or remembered, and never prints its fingerprint.
#[test]
fn the_status_line_names_the_paired_machine_and_no_fingerprint() {
    let connection = RemoteConnection {
        port: 8180,
        base_url: "http://127.0.0.1:8180/v1".to_owned(),
        ticket_fingerprint: FINGERPRINT.to_owned(),
        path: "direct".to_owned(),
        away_for_s: None,
    };
    let status = |connected: Option<RemoteConnection>| RemoteStatus {
        connected,
        stored_ticket_fingerprint: Some(FINGERPRINT.to_owned()),
        paired_name: Some("desk".to_owned()),
        has_remote_key: true,
        ..RemoteStatus::default()
    };
    let away = RemoteConnection {
        away_for_s: Some(120),
        ..connection.clone()
    };
    let lines = [
        connection_line(&status(Some(connection))),
        connection_line(&status(Some(away))),
        connection_line(&status(None)),
    ];
    assert_eq!(
        lines[0],
        "  Connected: desk  at http://127.0.0.1:8180/v1  (direct)"
    );
    assert!(
        lines[1].starts_with("  Connected: desk  at "),
        "{}",
        lines[1]
    );
    assert_eq!(
        lines[2],
        "  Connected: no \u{2014} `gglib remote join` dials desk again"
    );
    for line in &lines {
        assert!(!line.contains(FINGERPRINT), "{line}");
    }
}
