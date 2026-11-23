//! The commands that wait out the Hub's rate limit, and those that
//! surface it, as typed.

use std::sync::Arc;

use clap::Parser as _;

use super::*;
use crate::parser::Cli;

/// `argv`'s command, as clap parses it.
fn command(argv: &[&str]) -> Commands {
    Cli::try_parse_from(argv)
        .unwrap_or_else(|e| panic!("{argv:?} parses: {e}"))
        .command
        .expect("a command")
}

/// A console that draws nothing.
fn console() -> Arc<CliConsole> {
    Arc::new(CliConsole::hidden())
}

#[test]
fn a_one_shot_command_that_asks_the_hub_waits() {
    for argv in [
        &["gglib", "up"][..],
        &["gglib", "up", "--yes"],
        &["gglib", "model", "search", "qwen"],
        &["gglib", "model", "browse"],
        &["gglib", "model", "download", "owner/repo", "--list-quants"],
        &["gglib", "model", "download", "owner/repo"],
        &["gglib", "model", "check-updates", "--all"],
        &["gglib", "model", "upgrade", "1"],
    ] {
        assert!(waits_on_the_hub(&command(argv)), "{argv:?}");
        assert!(
            matches!(on_hub_limit(&command(argv), &console()), OnLimit::Wait(_)),
            "{argv:?}"
        );
    }
}

#[test]
fn a_command_that_answers_requests_or_asks_no_hub_surfaces() {
    for argv in [
        &["gglib", "daemon", "run"][..],
        &["gglib", "web"],
        &["gglib", "serve", "1"],
        &["gglib", "proxy"],
        &["gglib", "chat", "1"],
        &["gglib", "mcp", "list"],
        &["gglib", "model", "list"],
        &["gglib", "model", "verify", "1"],
    ] {
        assert!(!waits_on_the_hub(&command(argv)), "{argv:?}");
        assert!(
            matches!(on_hub_limit(&command(argv), &console()), OnLimit::Surface),
            "{argv:?}"
        );
    }
}
