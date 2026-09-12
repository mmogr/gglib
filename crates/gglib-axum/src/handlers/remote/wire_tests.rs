//! Tests for what the remote tunnel is asked.
//!
//! A `#[path]` sibling rather than an inline `mod tests`, because the two
//! together crossed the 300-line budget `scripts/check_rust_complexity.sh`
//! allows and the shapes are what the file is for. The subject here is what
//! a body means when it is empty, and what an older client's body still
//! means; `status_tests.rs` has the answers.

use super::*;

#[test]
fn an_empty_body_is_the_safe_default() {
    let req = RemoteEnableBody::default().into_request();
    assert!(!req.allow_mcp);
    assert!(req.relay.is_none());
    assert!(req.discovery, "discovery is on unless switched off");
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
fn an_empty_connect_body_reuses_the_last_ticket_on_a_free_port() {
    let req = RemoteConnectBody::default().into_request();
    assert!(req.pairing.is_none());
    assert!(req.port.is_none());
    assert!(req.discovery);
}
