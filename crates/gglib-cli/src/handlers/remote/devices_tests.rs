//! What a roster row, and `forget`'s help, read as in the terminal.

use super::*;

/// A row as the daemon sends it, described.
fn described() -> RemoteDevice {
    RemoteDevice {
        id: "dev-0a1b2c3d".to_owned(),
        label: Some("Matt's iPhone".to_owned()),
        joined_at: 1_757_000_000_000,
        redeemed_at: Some(1_757_000_060_000),
        last_seen: Some(1_757_000_120_000),
        peer: Some("3ca82708b995".to_owned()),
        admitted: None,
        recorded: true,
        description: "last seen 2m ago · paired from 3ca82708b995 · tunnel down".to_owned(),
        joined: true,
    }
}

/// The terminal prints what the daemon said of a row, word for word, beside
/// the name, and adds no words of its own.
#[test]
fn a_row_prints_the_daemons_description_word_for_word() {
    assert_eq!(
        line(&described()),
        "Matt's iPhone  (last seen 2m ago · paired from 3ca82708b995 · tunnel down)"
    );
}

/// A daemon older than the description sends none, and the row is the name
/// alone rather than a name and an empty pair of brackets.
#[test]
fn a_row_from_a_daemon_that_sends_no_description_prints_the_name_alone() {
    let older: RemoteDevice =
        serde_json::from_str(r#"{"id":"dev-0a1b2c3d","label":"Matt's iPhone","joined_at":0}"#)
            .expect("a row an older daemon sends");
    assert_eq!(line(&older), "Matt's iPhone");

    let unnamed = RemoteDevice {
        label: None,
        ..older
    };
    assert_eq!(line(&unnamed), "\u{2014}");
}

/// An id that is not one is refused here rather than sent.
///
/// This is not tidiness. The id is interpolated into a request path, and an
/// HTTP client resolves `..` the way a browser does — so `../../models/7`
/// would not fail, it would `DELETE` a *model*. Every id this machine mints
/// is `dev-` and eight hex digits; everything below is something only a
/// person's shell produces.
#[test]
fn an_id_that_could_leave_the_devices_route_is_not_a_device_id() {
    for escape in [
        "../../models/7",
        "..",
        "dev-0a1b2c3d/../../mcp/servers/3",
        "dev 0a1b2c3d",
        "dev-0a1b2c3d?force=1",
        "dev-0a1b2c3d#x",
        "",
    ] {
        assert!(
            !is_device_id(escape),
            "this reaches a route nobody named: {escape:?}"
        );
    }
}

/// And the ones that are, are — including the punctuation modelpipe allows
/// in a token name, so a future id shape is not refused by this check.
#[test]
fn the_ids_this_machine_mints_are_device_ids() {
    for ok in ["dev-0a1b2c3d", "dev-00000000", "a", "A.b_c-9"] {
        assert!(is_device_id(ok), "{ok:?}");
    }
}

/// [#1036]: `forget --help` is where someone looks for a way to cut devices
/// off, and it pointed them at the endpoint identity, whose deletion retires
/// the address and revokes no device's key.
///
/// Whitespace is folded because clap joins a doc comment's lines itself.
///
/// [#1036]: https://github.com/mmogr/gglib/issues/1036
#[test]
fn forget_help_says_deleting_the_identity_retires_the_address_not_a_device() {
    use clap::CommandFactory as _;

    let mut cli = crate::Cli::command();
    let forget = cli
        .find_subcommand_mut("remote")
        .and_then(|remote| remote.find_subcommand_mut("forget"))
        .expect("`gglib remote forget` exists");
    let help = forget.render_long_help().to_string();
    let help = help.split_whitespace().collect::<Vec<_>>().join(" ");

    assert!(
        !help.contains("revokes everything"),
        "the claim that deleting the identity revokes every device is gone: {help}"
    );
    assert!(
        help.contains("retires this machine's address and revokes no device"),
        "what replaced it says what the deletion does: {help}"
    );
}
