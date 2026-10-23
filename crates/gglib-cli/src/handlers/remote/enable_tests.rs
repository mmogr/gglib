//! Tests for the copy `gglib remote enable` prints.
//!
//! A `#[path]` sibling rather than an inline `mod tests`, because the two
//! together would cross the 300-line budget, the split `disable.rs` and its
//! tests were made along.

use clap::Parser as _;

use super::{Arming, EnableArgs, INVITE_NOTICE, RESUMED_NOTICE, body_of, mcp_notice, took};
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
/// key notice and `--allow-mcp` advice with it ([#1037]).
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
    assert!(
        RESUMED_NOTICE.contains("the MCP servers it opens among them"),
        "{RESUMED_NOTICE}"
    );
    assert!(
        !RESUMED_NOTICE.contains("now requires"),
        "the key notice belongs to the enable that armed the session: {RESUMED_NOTICE}"
    );
}

fn names(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

/// A session that opens servers is named by them, every time, and `/mcp`
/// is said to stay shut whatever was opened.
#[test]
fn the_notice_names_the_servers_a_session_opens() {
    for arming in [Arming::Enable, Arming::Invite, Arming::Resumed] {
        let said = mcp_notice(&names(&["tavily", "fs"]), arming);
        assert!(said.contains("may use: tavily, fs."), "{said}");
        assert!(said.contains("/mcp is never reachable"), "{said}");
    }
}

/// With none open, `enable` says how to open one, and a session it did not
/// arm says how to change it, and that the old switch opened nothing.
#[test]
fn with_none_open_the_notice_says_how_to_open_one() {
    let enabled = mcp_notice(&[], Arming::Enable);
    assert!(enabled.contains("pass --allow-mcp <server>"), "{enabled}");
    for arming in [Arming::Invite, Arming::Resumed] {
        let said = mcp_notice(&[], arming);
        assert!(
            said.contains("`disable`, then `enable --allow-mcp <server>`"),
            "{said}"
        );
        assert!(said.contains("old switch comes back with none"), "{said}");
    }
}

/// `--allow-mcp` took when it named, trimmed, the set the session opens, in
/// any order, and did not when the session opens another set.
#[test]
fn the_flag_took_only_when_it_named_the_set_the_session_opens() {
    assert!(
        took(&[], &names(&["tavily"])),
        "naming none asks for nothing"
    );
    assert!(took(&names(&[" fs", "tavily"]), &names(&["tavily", "fs"])));
    assert!(!took(&names(&["tavily"]), &[]));
    assert!(!took(&names(&["tavily"]), &names(&["tavily", "fs"])));
    assert!(!took(&names(&["tavily", "fs"]), &names(&["tavily"])));
}

/// `--allow-mcp` repeats, every name arrives in the order typed, and the
/// body sent to the daemon carries them all.
#[test]
fn every_allow_mcp_reaches_the_body_in_order() {
    let argv = [
        "gglib",
        "remote",
        "enable",
        "--allow-mcp",
        "a",
        "--allow-mcp",
        "b",
    ];
    let cli = crate::Cli::try_parse_from(argv).unwrap_or_else(|e| panic!("{argv:?}: {e}"));
    let Some(Commands::Remote {
        command: RemoteCommand::Enable { allow_mcp, .. },
    }) = cli.command
    else {
        panic!("{argv:?} is not `remote enable`");
    };
    assert_eq!(allow_mcp, names(&["a", "b"]));

    let body = body_of(&EnableArgs {
        allow_mcp,
        ..EnableArgs::default()
    });
    assert_eq!(body.mcp_servers, names(&["a", "b"]));
}
