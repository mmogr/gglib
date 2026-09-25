//! Tests for the command line of `gglib remote join`: the only name the
//! dial to another machine answers to.
//!
//! Parsed rather than run, because running `join` needs a daemon and a far
//! machine, and what these guard is the parser's half.

use clap::Parser as _;
use clap::error::ErrorKind;

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
