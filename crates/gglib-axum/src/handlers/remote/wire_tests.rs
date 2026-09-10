//! Tests for the remote tunnel's request and response shapes.
//!
//! A `#[path]` sibling rather than an inline `mod tests`, because the two
//! together crossed the 300-line budget `scripts/check_rust_complexity.sh`
//! allows and the shapes are what the file is for. The conversions are the
//! subject here: what a default body means, and what each snapshot turns
//! into on the wire.

use super::*;

#[test]
fn an_empty_body_is_the_safe_default() {
    let req = RemoteEnableBody::default().into_request();
    assert!(!req.allow_mcp);
    assert!(req.relay.is_none());
    assert!(req.discovery, "discovery is on unless switched off");
    assert!(
        !req.keep_identity,
        "a body that says nothing must not leave a key on the machine"
    );
}

/// An old client posts a body with no `keep_identity` at all. It has to
/// mean the same thing as `false`, or upgrading the daemon would start
/// writing a key to disk for callers that never asked for one.
#[test]
fn a_body_written_before_the_flag_existed_keeps_no_key() {
    let body: RemoteEnableBody =
        serde_json::from_str(r#"{"allow_mcp":false,"discovery":true}"#).unwrap();
    assert!(!body.into_request().keep_identity);
}

#[test]
fn keeping_the_identity_is_carried_through() {
    let body: RemoteEnableBody = serde_json::from_str(r#"{"keep_identity":true}"#).unwrap();
    let req = body.into_request();
    assert!(req.keep_identity);
    assert!(req.discovery, "the two switches are independent");
}

#[test]
fn discovery_off_is_carried_through() {
    let body: RemoteEnableBody =
        serde_json::from_str(r#"{"allow_mcp":true,"discovery":false}"#).unwrap();
    let req = body.into_request();
    assert!(req.allow_mcp);
    assert!(!req.discovery);
}

#[test]
fn an_empty_connect_body_reuses_the_last_ticket_on_a_free_port() {
    let req = RemoteConnectBody::default().into_request();
    assert!(req.pairing.is_none());
    assert!(req.port.is_none());
    assert!(req.discovery);
}

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
