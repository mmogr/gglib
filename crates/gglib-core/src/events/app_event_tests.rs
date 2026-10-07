//! Tests for [`AppEvent`](super::AppEvent) — serialization shape.
//!
//! Split out of `mod.rs` when the remote-tunnel variants arrived and the
//! file reached its budget.

use super::*;

#[test]
fn test_event_serialization() {
    let event = AppEvent::server_started(1, "Llama-2-7B", 8080);
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("\"type\":\"server_started\""));
    assert!(json.contains("\"modelName\":\"Llama-2-7B\""));
    assert!(json.contains("\"port\":8080"));
}

/// The remote events carry a fingerprint and never a ticket.
#[test]
fn remote_event_shape() {
    let enabled = AppEvent::remote_enabled("3ca82708b995".to_owned());
    let json = serde_json::to_string(&enabled).unwrap();
    assert!(json.contains("\"type\":\"remote_enabled\""), "{json}");
    assert!(
        json.contains("\"ticketFingerprint\":\"3ca82708b995\""),
        "{json}"
    );
    let joined = AppEvent::remote_joined(8081);
    let json = serde_json::to_string(&joined).unwrap();
    assert!(json.contains("\"type\":\"remote_joined\""), "{json}");
}
