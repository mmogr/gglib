//! Tests for the copy `gglib remote enable` prints.
//!
//! A `#[path]` sibling rather than an inline `mod tests`, because the two
//! together would cross the 300-line budget, the split `disable.rs` and its
//! tests were made along.

use clap::Parser as _;

use super::{
    ALLOW_MCP_GONE, EnableArgs, INVITE_NOTICE, RESUMED_NOTICE, body_of, mcp_notice,
    refuse_allow_mcp,
};
use crate::commands::{Commands, RemoteCommand};

/// An invite claims the code it offered, not a device that may never come:
/// the notice follows "expired and nobody paired" as readily as "paired",
/// so the old claim was false whenever nobody came.
#[test]
fn the_invite_notice_claims_a_code_and_not_a_device() {
    assert!(!INVITE_NOTICE.contains("added a device"), "{INVITE_NOTICE}");
    assert!(
        INVITE_NOTICE.contains("offered a code for one more device"),
        "{INVITE_NOTICE}"
    );
    assert!(
        INVITE_NOTICE.contains("changed nothing else about the session"),
        "{INVITE_NOTICE}"
    );
}

/// A plain `enable` that waited for the daemon's own resume was answered from
/// a session it did not arm. It says the session came back and changed
/// nothing, rather than that the switch was just thrown, which would bring the
/// key notice with it ([#1037]). It says nothing of the MCP servers, which
/// no `enable` changes.
///
/// [#1037]: https://github.com/mmogr/gglib/issues/1037
#[test]
fn the_resumed_notice_says_the_session_came_back_and_changed_nothing() {
    assert!(
        RESUMED_NOTICE.contains("already coming back up"),
        "{RESUMED_NOTICE}"
    );
    assert!(
        RESUMED_NOTICE.contains("changed nothing about the session"),
        "{RESUMED_NOTICE}"
    );
    assert!(
        RESUMED_NOTICE.contains("`gglib remote disable`"),
        "{RESUMED_NOTICE}"
    );
    assert!(!RESUMED_NOTICE.contains("MCP"), "{RESUMED_NOTICE}");
    assert!(
        !RESUMED_NOTICE.contains("now requires"),
        "the key notice belongs to the enable that armed the session: {RESUMED_NOTICE}"
    );
}

fn names(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

/// The servers open to paired devices are named, every time, and `/mcp` is
/// said to stay shut whatever is open.
#[test]
fn the_notice_names_the_servers_open_to_devices() {
    let said = mcp_notice(&names(&["tavily", "fs"]));
    assert!(said.contains("may use: tavily, fs."), "{said}");
    assert!(said.contains("/mcp is never reachable"), "{said}");
}

/// With none open, the notice says how to open one, whichever command
/// printed it: a server is opened by a flag on it, never by `enable`.
#[test]
fn with_none_open_the_notice_says_how_to_open_one() {
    let said = mcp_notice(&[]);
    assert_eq!(
        said,
        "  No MCP server is open to paired devices; `gglib mcp allow-remote <server>` opens one."
    );
    assert!(!said.contains("--allow-mcp"), "{said}");
}

/// `--allow-mcp` still parses, hidden, and is refused with where the grant
/// went, before anything else runs; without it nothing is refused, and the
/// body sent to the daemon names no server.
#[test]
fn allow_mcp_is_refused_with_a_pointer_and_the_body_names_no_server() {
    let argv = ["gglib", "remote", "enable", "--allow-mcp", "a"];
    let cli = crate::Cli::try_parse_from(argv).unwrap_or_else(|e| panic!("{argv:?}: {e}"));
    let Some(Commands::Remote {
        command: RemoteCommand::Enable { allow_mcp, .. },
    }) = cli.command
    else {
        panic!("{argv:?} is not `remote enable`");
    };
    let refused = refuse_allow_mcp(&allow_mcp).expect_err("refused");
    assert_eq!(refused.to_string(), ALLOW_MCP_GONE);
    assert!(ALLOW_MCP_GONE.contains("`gglib mcp allow-remote <names>`"));
    assert!(refuse_allow_mcp(&[]).is_ok());

    let body = serde_json::to_value(body_of(&EnableArgs::default())).expect("serialise");
    assert!(body.get("mcp_servers").is_none(), "{body}");
}

/// The flag is hidden from the help: it is there only to say it is gone.
#[test]
fn allow_mcp_is_not_in_the_help() {
    use clap::CommandFactory as _;

    let mut cli = crate::Cli::command();
    let enable = cli
        .find_subcommand_mut("remote")
        .and_then(|remote| remote.find_subcommand_mut("enable"))
        .expect("`remote enable`");
    let help = enable.render_long_help().to_string();
    assert!(!help.contains("--allow-mcp"), "{help}");
}
