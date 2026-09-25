//! Tests for the enable and join exchanges: what a body means when it is
//! empty, what an older client's body still means, and what the enable
//! answer carries. `wire_tests.rs` has the status.

use super::*;

#[test]
fn an_empty_body_is_the_safe_default() {
    let req = RemoteEnableBody::default().into_request();
    assert!(!req.allow_mcp);
    assert!(req.relay.is_none());
    assert!(req.discovery, "discovery is on unless switched off");
}

/// Every body and answer of the two exchanges that says nothing still reads,
/// every field at its default.
#[test]
fn an_exchange_that_says_nothing_still_reads() {
    let enable_body: RemoteEnableBody = serde_json::from_str("{}").expect("an empty enable body");
    let join_body: RemoteJoinBody = serde_json::from_str("{}").expect("an empty join body");
    let enabled: RemoteEnableResponse =
        serde_json::from_str("{}").expect("an enable answer that says nothing");
    let joined: RemoteJoinResponse =
        serde_json::from_str("{}").expect("a join answer that says nothing");

    assert_eq!(enable_body, RemoteEnableBody::default());
    assert_eq!(join_body, RemoteJoinBody::default());
    assert_eq!(
        enabled,
        RemoteEnableResponse {
            ticket: String::new(),
            code: None,
            pairing: None,
            expires_in_s: None,
            device: None,
            mcp_allowed: false,
            already_up: false,
        }
    );
    assert_eq!(
        joined,
        RemoteJoinResponse {
            port: 0,
            base_url: String::new(),
            ticket_fingerprint: String::new(),
            paired: false,
            moved_from: None,
        }
    );
}

/// A client built against the old shape still gets a 200.
///
/// `keep_identity` asked for what every `enable` now does anyway, so the
/// field is accepted and dropped rather than rejected: a desktop app or
/// script that still sends it must not get a 422 for a word that stopped
/// meaning anything. Both values are tested because both must be inert —
/// `false` especially, since it used to mean "mint a throwaway identity"
/// and must no longer be able to ask for one.
#[test]
fn a_body_that_still_sends_keep_identity_is_accepted_and_the_flag_ignored() {
    for sent in ["true", "false"] {
        let body: RemoteEnableBody =
            serde_json::from_str(&format!(r#"{{"keep_identity":{sent}}}"#))
                .expect("a body from an older client still deserialises");
        let req = body.into_request();
        assert!(
            req.discovery,
            "keep_identity={sent} must not disturb anything else"
        );
        assert!(!req.allow_mcp, "keep_identity={sent} is not a grant");
    }
}

/// A body written before the flag existed, and one written after it is gone.
#[test]
fn a_body_without_the_flag_is_unremarkable() {
    let body: RemoteEnableBody =
        serde_json::from_str(r#"{"allow_mcp":false,"discovery":true}"#).unwrap();
    let req = body.into_request();
    assert!(req.discovery);
    assert!(!req.allow_mcp);
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
fn an_empty_join_body_reuses_the_last_ticket_on_a_free_port() {
    let req = RemoteJoinBody::default().into_request();
    assert!(req.pairing.is_none());
    assert!(req.port.is_none());
    assert!(req.discovery);
}

/// An enable answer that offered a code, as the daemon's JSON has it, with
/// `device` set or left out.
fn enable_answer(device: Option<&str>) -> serde_json::Value {
    let mut answer = serde_json::json!({
        "ticket": "pipeadlvvgabqkyqvn6vjp7nhslea45a5yls6pnkmizfv4bbu2hxa5iruaaauhlp2na",
        "code": "483920",
        "pairing": "pipeadlvvgabqkyqvn6vjp7nhslea45a5yls6pnkmizfv4bbu2hxa5iruaaauhlp2na-483920",
        "expires_in_s": 120,
        "mcp_allowed": false,
        "already_up": false,
    });
    if let Some(device) = device {
        answer["device"] = device.into();
    }
    answer
}

/// The answer that hands out a code names the device the code will issue a
/// key to, and the CLI reads it back: it is how the pairing screen names what
/// paired.
#[test]
fn the_enable_answer_carries_the_device_the_code_is_for() {
    let got: RemoteEnableResponse = serde_json::from_value(enable_answer(Some("dev-a1b2c3d4")))
        .expect("the answer the daemon sends");

    assert_eq!(got.device.as_deref(), Some("dev-a1b2c3d4"));
}

/// A daemon that predates per-device keys names no device, and its answer
/// still reads, code and all.
#[test]
fn an_enable_answer_without_a_device_still_reads() {
    let got: RemoteEnableResponse =
        serde_json::from_value(enable_answer(None)).expect("an answer with no device still reads");

    assert_eq!(got.device, None);
    assert_eq!(got.code.as_deref(), Some("483920"));
}
