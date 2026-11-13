//! What the tunnel's state reads as, for each state the serve side and the
//! connect side can be in.

use super::super::wire::{RemoteConnection, RemoteDevice, RemoteStatus};
use super::RemoteStatusLines;

/// A roster row, which is all "paired" counts.
fn device(id: &str) -> RemoteDevice {
    RemoteDevice {
        id: id.to_owned(),
        label: None,
        joined_at: 1,
        redeemed_at: Some(1),
        last_seen: None,
        peer: None,
        admitted: Some(true),
        recorded: true,
        description: String::new(),
        joined: true,
    }
}

fn connection(away_for_s: Option<u64>) -> RemoteConnection {
    RemoteConnection {
        port: 8180,
        base_url: "http://127.0.0.1:8180/v1".to_owned(),
        ticket_fingerprint: "e5a1b0c2d3f4".to_owned(),
        path: "relayed".to_owned(),
        away_for_s,
    }
}

fn lines(status: RemoteStatus) -> RemoteStatusLines {
    let status = status.with_lines();
    RemoteStatusLines {
        sharing: status.sharing,
        sharing_note: status.sharing_note,
        reach: status.reach_line,
        reach_note: status.reach_note,
    }
}

#[test]
fn off_is_not_sharing_with_nothing_more_to_say() {
    let said = lines(RemoteStatus::default());
    assert_eq!(said.sharing, "Not sharing");
    assert_eq!(said.sharing_note, "");
    assert_eq!(said.reach, "");
}

/// The switch on with nothing bound has a sentence of its own: "not
/// sharing" would send someone to switch on a thing already on.
#[test]
fn switched_on_with_nothing_bound_says_so_in_full() {
    let said = lines(RemoteStatus {
        remote_enabled: true,
        ..RemoteStatus::default()
    });
    assert_eq!(said.sharing, "Switched on, nothing bound");
    assert_eq!(
        said.sharing_note,
        "Switched on, but nothing is bound \u{2014} still arming, or it could not reach a relay."
    );
}

/// Sharing counts the roster's rows, none, one or many.
#[test]
fn sharing_says_how_many_are_paired() {
    for (devices, said) in [
        (vec![], "Sharing \u{b7} 0 paired"),
        (vec![device("a")], "Sharing \u{b7} 1 paired"),
        (
            vec![device("a"), device("b"), device("c")],
            "Sharing \u{b7} 3 paired",
        ),
    ] {
        let status = RemoteStatus {
            enabled: true,
            remote_enabled: true,
            devices,
            ..RemoteStatus::default()
        };
        let got = lines(status);
        assert_eq!(got.sharing, said);
        assert_eq!(got.sharing_note, "");
    }
}

/// An invite nobody took is a roster row, but no device is paired by it.
#[test]
fn an_untaken_invite_is_not_counted_as_paired() {
    let untaken = RemoteDevice {
        redeemed_at: None,
        joined: false,
        ..device("invited")
    };
    let status = RemoteStatus {
        enabled: true,
        remote_enabled: true,
        devices: vec![untaken.clone()],
        ..RemoteStatus::default()
    };
    assert_eq!(lines(status).sharing, "Sharing \u{b7} 0 paired");
    let status = RemoteStatus {
        enabled: true,
        remote_enabled: true,
        devices: vec![device("a"), untaken],
        ..RemoteStatus::default()
    };
    assert_eq!(lines(status).sharing, "Sharing \u{b7} 1 paired");
}

/// The joined machine by its name, or in words when it gave none; never by
/// its fingerprint. Not joined, there is no reach line, whatever is stored.
#[test]
fn the_reach_line_names_the_joined_machine() {
    let said = |away_for_s, name: Option<&str>| {
        lines(RemoteStatus {
            connected: Some(connection(away_for_s)),
            paired_name: name.map(str::to_owned),
            ..RemoteStatus::default()
        })
    };
    let joined = |away_for_s, name| said(away_for_s, name).reach;
    assert_eq!(
        joined(None, Some("desk")),
        "desk \u{b7} Connected \u{b7} relayed"
    );
    // Away ends at the word, so a surface's "since" or "for" follows it;
    // what becomes of the machine is its own line, and only while away.
    let away = said(Some(120), Some("desk"));
    assert_eq!(away.reach, "desk \u{b7} Away");
    assert_eq!(
        away.reach_note,
        "The address stays, and it reconnects when that machine is back."
    );
    assert_eq!(said(None, Some("desk")).reach_note, "");
    assert_eq!(
        joined(None, None),
        "the paired machine \u{b7} Connected \u{b7} relayed"
    );
    assert!(!joined(None, Some("desk")).contains("e5a1b0c2d3f4"));

    let remembered = lines(RemoteStatus {
        stored_ticket_fingerprint: Some("e5a1b0c2d3f4".to_owned()),
        paired_name: Some("desk".to_owned()),
        has_remote_key: true,
        ..RemoteStatus::default()
    });
    assert_eq!(remembered.reach, "");
    assert_eq!(remembered.reach_note, "");
}

/// The lines are said where a status is read, from its fields: lines in the
/// answer itself are never read.
#[test]
fn lines_in_an_answer_are_said_again_not_read() {
    let json = serde_json::json!({
        "enabled": false,
        "sharing": "forged",
        "sharing_note": "forged",
        "reach_line": "forged",
        "reach_note": "forged"
    });
    let read: RemoteStatus = serde_json::from_value(json).expect("a status");
    assert_eq!(
        (
            read.sharing.as_str(),
            read.sharing_note.as_str(),
            read.reach_line.as_str()
        ),
        ("", "", "")
    );
    assert_eq!(read.with_lines().sharing, "Not sharing");
}
