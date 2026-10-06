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
    use crate::download::{
        DownloadId, DownloadOutcome, FinishedDownload, QueueRunSummary, QueueSnapshot,
    };

    let ended = |outcome| {
        DownloadEvent::ended(&FinishedDownload::new(
            &DownloadId::from_model("id"),
            outcome,
        ))
    };

    let summary = QueueRunSummary {
        run_id: uuid::Uuid::nil(),
        started_at_ms: 0,
        completed_at_ms: 0,
        total_attempts_downloaded: 0,
        total_attempts_failed: 0,
        total_attempts_cancelled: 0,
        unique_models_downloaded: 0,
        unique_models_failed: 0,
        unique_models_cancelled: 0,
        truncated: false,
        items: Vec::new(),
    };
    let cases = [
        (
            DownloadEvent::queue_snapshot(QueueSnapshot::default()),
            "download:queue_snapshot",
        ),
        (
            ended(DownloadOutcome::Completed { message: None }),
            "download:completed",
        ),
        (
            ended(DownloadOutcome::Failed {
                error: "error".to_string(),
            }),
            "download:failed",
        ),
        (ended(DownloadOutcome::Cancelled), "download:cancelled"),
        (
            DownloadEvent::queue_run_complete(summary),
            "download:queue_run_complete",
        ),
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
    let joined = AppEvent::remote_joined(8081);
    assert_eq!(joined.event_name(), "remote:joined");
    let json = serde_json::to_string(&joined).unwrap();
    assert!(json.contains("\"type\":\"remote_joined\""), "{json}");
    assert_eq!(
        AppEvent::remote_disconnected().event_name(),
        "remote:disconnected"
    );
    assert_eq!(AppEvent::remote_away(8180).event_name(), "remote:away");
    assert_eq!(AppEvent::remote_back(8180).event_name(), "remote:back");
}
