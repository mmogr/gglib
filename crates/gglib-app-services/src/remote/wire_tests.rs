//! Tests for what `GET /api/remote/status` says, and does not, and for what
//! a `forget` answers.
//!
//! The subject here is the one a `GET` makes dangerous: a status is the
//! response anything on this machine can ask for twice, so what it leaves
//! out is as much the contract as what it carries. And the one a shared shape
//! makes possible: the CLI reads what the daemon writes with this same type,
//! so an answer that lacks a field has to read.

use super::*;

/// A device row with every field set.
fn row() -> RemoteDevice {
    RemoteDevice {
        id: "dev-0a1b2c3d".to_owned(),
        label: Some("Matt's iPhone".to_owned()),
        joined_at: 1_757_000_000_000,
        redeemed_at: Some(1_757_000_060_000),
        last_seen: Some(1_757_000_120_000),
        peer: Some("3ca82708b995".to_owned()),
        admitted: Some(true),
        recorded: true,
        description: "last seen 2m ago · paired from 3ca82708b995".to_owned(),
        joined: true,
    }
}

/// The sorted keys of a JSON object.
fn keys(value: &serde_json::Value) -> Vec<&str> {
    let mut keys: Vec<&str> = value
        .as_object()
        .expect("a JSON object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    keys
}

/// The status never carries the ticket, whatever else it holds.
#[test]
fn the_status_has_no_field_that_could_hold_a_ticket() {
    let status = RemoteStatus {
        enabled: true,
        ticket_fingerprint: Some("3ca82708b995".to_owned()),
        stored_ticket_fingerprint: Some("aabbccddeeff".to_owned()),
        ..RemoteStatus::default()
    };
    let json = serde_json::to_string(&status).unwrap();
    assert!(json.contains("\"ticket_fingerprint\":\"3ca82708b995\""));
    assert!(!json.contains("\"ticket\":"), "{json}");
    assert!(!json.contains("pipe"), "{json}");
}

/// The roster reaches the status surface, and carries no key when it does.
///
/// `GET` is the verb anything can call twice, and this one is polled, so the
/// property is worth a test rather than an inspection. The row's own id is
/// not a secret: it travels on every tunnelled request as
/// `X-Modelpipe-Device`.
#[test]
fn the_status_carries_the_roster_and_none_of_its_keys() {
    let status = RemoteStatus {
        enabled: true,
        devices: vec![row()],
        ..RemoteStatus::default()
    };

    let json = serde_json::to_value(&status).unwrap();
    let device = &json["devices"][0];
    assert_eq!(device["id"], "dev-0a1b2c3d");
    assert_eq!(device["admitted"], true);
    assert_eq!(device["recorded"], true);
    assert_eq!(device["peer"], "3ca82708b995");
    // The exact field set, not a search for "key" and "token": a new
    // `api_key` or `secret` field would pass a substring test, and a new
    // field on a polled `GET` is a contract change whatever it is called.
    assert_eq!(
        keys(device),
        [
            "admitted",
            "description",
            "id",
            "joined",
            "joined_at",
            "label",
            "last_seen",
            "peer",
            "recorded",
            "redeemed_at"
        ],
        "{json}"
    );
}

/// The status's own field set, pinned for the same reason as a row's.
#[test]
fn the_status_sends_exactly_its_contract_fields() {
    let json = serde_json::to_value(RemoteStatus::default()).unwrap();
    assert_eq!(
        keys(&json),
        [
            "connected",
            "devices",
            "enabled",
            "has_remote_key",
            "identity_path",
            "last_peer",
            "last_tunnelled_ms",
            "mcp_allowed",
            "paired",
            "pairing_active",
            "path",
            "peers",
            "remote_enabled",
            "stored_ticket_fingerprint",
            "ticket_fingerprint",
            "tunnelled_requests"
        ],
        "{json}"
    );
}

/// A machine that has issued no keys answers with an empty list, not `null`.
///
/// The surface iterates it, and a missing list would read as a machine that
/// has paired nothing.
#[test]
fn a_machine_with_no_devices_still_answers_with_a_list() {
    let json = serde_json::to_string(&RemoteStatus::default()).unwrap();
    assert!(json.contains("\"devices\":[]"), "{json}");
}

/// A status that says only whether the serve side is up still reads, with
/// every other field at its default; one that does not say even that is not
/// a status.
#[test]
fn a_status_with_only_enabled_still_reads() {
    let status: RemoteStatus =
        serde_json::from_str(r#"{"enabled":true}"#).expect("every field but enabled has a default");
    assert_eq!(
        status,
        RemoteStatus {
            enabled: true,
            ..RemoteStatus::default()
        }
    );

    assert!(serde_json::from_str::<RemoteStatus>("{}").is_err());
}

/// A row that names only its device reads as a roster row with nothing else
/// known of it, not as a key held with no record: `recorded` is the one field
/// whose absence reads as true.
#[test]
fn a_row_with_only_an_id_reads_as_a_roster_row() {
    let device: RemoteDevice =
        serde_json::from_str(r#"{"id":"dev-0a1b2c3d"}"#).expect("every field but id has a default");

    assert!(device.recorded, "a row that does not say is a roster row");
    assert_eq!(
        (device.description.as_str(), device.joined, device.admitted),
        ("", false, None)
    );
    assert!(serde_json::from_str::<RemoteDevice>("{}").is_err());
}

/// What the daemon writes, the CLI reads back unchanged: every field of a
/// status with both sides up and a device on it.
#[test]
fn a_status_reads_back_as_the_status_that_was_sent() {
    let sent = RemoteStatus {
        enabled: true,
        ticket_fingerprint: Some("3ca82708b995".to_owned()),
        pairing_active: true,
        paired: true,
        path: Some("direct".to_owned()),
        peers: vec![RemotePeer {
            fingerprint: "ba9876543210".to_owned(),
            path: "relayed".to_owned(),
        }],
        mcp_allowed: true,
        tunnelled_requests: 7,
        last_tunnelled_ms: Some(1_757_000_120_000),
        last_peer: Some("ba9876543210".to_owned()),
        connected: Some(RemoteConnection {
            port: 8181,
            base_url: "http://127.0.0.1:8181/v1".to_owned(),
            ticket_fingerprint: "aabbccddeeff".to_owned(),
            path: "direct".to_owned(),
            away_for_s: Some(40),
        }),
        stored_ticket_fingerprint: Some("aabbccddeeff".to_owned()),
        has_remote_key: true,
        remote_enabled: true,
        identity_path: Some("/data/remote/identity.key".to_owned()),
        devices: vec![
            row(),
            RemoteDevice {
                label: None,
                ..row()
            },
        ],
    };

    let json = serde_json::to_string(&sent).unwrap();
    let read: RemoteStatus = serde_json::from_str(&json).unwrap();

    assert_eq!(read, sent);
}

/// A peer with every field at its default.
fn no_peer() -> RemotePeer {
    RemotePeer {
        fingerprint: String::new(),
        path: String::new(),
    }
}

/// A connection with every field at its default.
fn no_connection() -> RemoteConnection {
    RemoteConnection {
        port: 0,
        base_url: String::new(),
        ticket_fingerprint: String::new(),
        path: String::new(),
        away_for_s: None,
    }
}

/// A peer, a connection and a forget answer that say nothing still read,
/// every field at its default, and so does a status that carries them.
#[test]
fn a_peer_a_connection_and_a_forget_answer_that_say_nothing_still_read() {
    let peer: RemotePeer = serde_json::from_str("{}").expect("every field has a default");
    let connection: RemoteConnection =
        serde_json::from_str("{}").expect("every field has a default");
    let forgotten: RemoteForgotten = serde_json::from_str("{}").expect("every field has a default");
    assert_eq!(peer, no_peer());
    assert_eq!(connection, no_connection());
    assert_eq!(forgotten, RemoteForgotten { forgotten: false });

    let status: RemoteStatus =
        serde_json::from_str(r#"{"enabled":true,"peers":[{}],"connected":{}}"#)
            .expect("a status whose peer and connection say nothing");
    assert_eq!(
        status,
        RemoteStatus {
            enabled: true,
            peers: vec![no_peer()],
            connected: Some(no_connection()),
            ..RemoteStatus::default()
        }
    );
}

/// A peer's and a connection's field sets, pinned for the same reason as a
/// row's: both ride the polled `GET`.
#[test]
fn a_peer_and_a_connection_send_exactly_their_contract_fields() {
    let peer = serde_json::to_value(no_peer()).unwrap();
    let connection = serde_json::to_value(no_connection()).unwrap();
    assert_eq!(keys(&peer), ["fingerprint", "path"], "{peer}");
    assert_eq!(
        keys(&connection),
        [
            "away_for_s",
            "base_url",
            "path",
            "port",
            "ticket_fingerprint"
        ],
        "{connection}"
    );
}
