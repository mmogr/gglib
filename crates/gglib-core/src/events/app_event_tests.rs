//! Tests for [`AppEvent`](super::AppEvent) — serialization shape and the
//! colon-separated names.
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

#[test]
fn test_event_names() {
    assert_eq!(
        AppEvent::server_started(1, "test", 8080).event_name(),
        "server:started"
    );
    assert_eq!(AppEvent::model_removed(1).event_name(), "model:removed");
    // The download names are covered exhaustively by
    // `download_event_names_are_stable` below.
}

/// Lock down the colon-separated download event names.
///
/// This guards [`AppEvent::event_name`] against silent renames, and that
/// is all it guards: none of these five strings appears anywhere in the
/// frontend. The names the GUI actually validates are the `snake_case`
/// serde variants, in `src/services/decoders/downloadEvent.ts`.
///
/// It was written when the frontend did subscribe to these, over the
/// Tauri bus, and its doc pointed at `eventNames.ts` until #833 deleted
/// that file. Pointing it at `getEventCategory` instead — as an earlier
/// pass here did — is no better: that allowlist matches the serde tag, so
/// updating it in response to this test failing would be a no-op.
///
/// Context: downloads started but the progress UI never appeared, because
/// the frontend listened for the wrong event names.
#[test]
fn download_event_names_are_stable() {
    let cases = [
        (DownloadEvent::started("id"), "download:started"),
        (
            DownloadEvent::progress("id", 50, 100, Some(1024.0), Some(10.0)),
            "download:progress",
        ),
        (
            DownloadEvent::completed("id", None::<String>),
            "download:completed",
        ),
        (DownloadEvent::failed("id", "error"), "download:failed"),
        (DownloadEvent::cancelled("id"), "download:cancelled"),
    ];

    for (event, expected_name) in cases {
        assert_eq!(AppEvent::Download { event }.event_name(), expected_name);
    }
}

/// The remote events carry a fingerprint and never a ticket, and their
/// names follow the proxy's.
#[test]
fn remote_event_names_and_shape() {
    let enabled = AppEvent::remote_enabled("3ca82708b995".to_owned());
    assert_eq!(enabled.event_name(), "remote:enabled");
    let json = serde_json::to_string(&enabled).unwrap();
    assert!(json.contains("\"type\":\"remote_enabled\""), "{json}");
    assert!(
        json.contains("\"ticketFingerprint\":\"3ca82708b995\""),
        "{json}"
    );
    assert_eq!(AppEvent::remote_disabled().event_name(), "remote:disabled");
    assert_eq!(AppEvent::remote_paired(None).event_name(), "remote:paired");
    assert_eq!(
        AppEvent::remote_connected(8081).event_name(),
        "remote:connected"
    );
    assert_eq!(
        AppEvent::remote_disconnected().event_name(),
        "remote:disconnected"
    );
}
