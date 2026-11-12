//! Tests for the copy `gglib remote enable` prints.
//!
//! A `#[path]` sibling rather than an inline `mod tests`, the house layout.

use clap::Parser as _;

use super::{EnableArgs, INVITE_NOTICE, RESUMED_NOTICE, body_of, mcp_notice};

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
/// a session it did not arm. It says remote access was already on and changed
/// nothing, rather than that the switch was just thrown, which would bring the
/// key notice with it ([#1037]), or that it is still coming back up, which
/// reads as a daemon still restarting ([#1534]). It says nothing of the MCP
/// servers, which no `enable` changes.
///
/// [#1037]: https://github.com/mmogr/gglib/issues/1037
/// [#1534]: https://github.com/mmogr/gglib/issues/1534
#[test]
fn the_resumed_notice_says_remote_access_was_already_on_and_changed_nothing() {
    assert!(
        RESUMED_NOTICE.contains("was already on"),
        "{RESUMED_NOTICE}"
    );
    assert!(
        !RESUMED_NOTICE.contains("coming back up"),
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

/// `enable` names no MCP server: `--allow-mcp` is not a flag, and the body
/// sent to the daemon names none. A server is opened by a flag on it.
#[test]
fn enable_names_no_mcp_server() {
    let argv = ["gglib", "remote", "enable", "--allow-mcp", "a"];
    assert!(crate::Cli::try_parse_from(argv).is_err(), "{argv:?}");

    let body = serde_json::to_value(body_of(&EnableArgs::default())).expect("serialise");
    assert!(body.get("mcp_servers").is_none(), "{body}");
}
