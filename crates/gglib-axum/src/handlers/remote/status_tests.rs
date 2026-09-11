//! Tests for what `GET /api/remote/status` says, and does not.
//!
//! The subject here is the one a `GET` makes dangerous: a status is the
//! response anything on this machine can ask for twice, so what it leaves
//! out is as much the contract as what it carries.

use super::*;

/// The status never carries the ticket, whatever the snapshot holds.
#[test]
fn the_status_has_no_field_that_could_hold_a_ticket() {
    let status = RemoteStatus::from(RemoteStatusSnapshot {
        enabled: true,
        ticket_fingerprint: Some("3ca82708b995".to_owned()),
        stored_ticket_fingerprint: Some("aabbccddeeff".to_owned()),
        ..RemoteStatusSnapshot::default()
    });
    let json = serde_json::to_string(&status).unwrap();
    assert!(json.contains("\"ticket_fingerprint\":\"3ca82708b995\""));
    assert!(!json.contains("\"ticket\":"), "{json}");
    assert!(!json.contains("pipe"), "{json}");
}

/// The roster reaches the status surface, and carries no key when it does.
///
/// The device rows on a status are the same shape `GET /api/remote/devices`
/// answers with, which has no field a key could live in — but `GET` is the
/// verb anything can call twice, and this one is polled, so the property is
/// worth a test rather than an inspection. The row's own id is not a secret:
/// it travels on every tunnelled request as `X-Modelpipe-Device`.
#[test]
fn the_status_carries_the_roster_and_none_of_its_keys() {
    let status = RemoteStatus::from(RemoteStatusSnapshot {
        enabled: true,
        devices: vec![gglib_app_services::DeviceView {
            id: "dev-0a1b2c3d".to_owned(),
            label: Some("Matt's iPhone".to_owned()),
            joined_at: 1_757_000_000_000,
            redeemed_at: Some(1_757_000_060_000),
            last_seen: None,
            admitted: Some(true),
        }],
        ..RemoteStatusSnapshot::default()
    });

    let json = serde_json::to_value(&status).unwrap();
    let row = json["devices"][0].as_object().expect("one device row");
    assert_eq!(row["id"], "dev-0a1b2c3d");
    assert_eq!(row["admitted"], true);
    // The exact field set, not a search for "key" and "token": a new
    // `api_key` or `secret` field would pass a substring test, and a new
    // field on a polled `GET` is a contract change whatever it is called.
    let mut fields: Vec<&str> = row.keys().map(String::as_str).collect();
    fields.sort_unstable();
    assert_eq!(
        fields,
        [
            "admitted",
            "id",
            "joined_at",
            "label",
            "last_seen",
            "redeemed_at"
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
    let json = serde_json::to_string(&RemoteStatus::from(RemoteStatusSnapshot::default())).unwrap();
    assert!(json.contains("\"devices\":[]"), "{json}");
}
