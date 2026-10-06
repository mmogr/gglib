//! The download events: their ids, the snapshot's shape, and an ending's
//! words.

use super::super::types::DownloadId;
use super::*;

fn ended(outcome: DownloadOutcome) -> FinishedDownload {
    FinishedDownload::new(&DownloadId::new("owner/zeta", Some("Q8_0")), outcome)
}

fn failed(error: &str) -> DownloadOutcome {
    DownloadOutcome::Failed {
        error: error.to_string(),
    }
}

#[test]
fn test_event_id_extraction() {
    let id = Some("owner/zeta:Q8_0");
    assert_eq!(DownloadEvent::ended(&ended(failed("e"))).id(), id);
    assert_eq!(
        DownloadEvent::ended(&ended(DownloadOutcome::Cancelled)).id(),
        id
    );
    let snapshot = DownloadEvent::queue_snapshot(QueueSnapshot::default());
    assert!(snapshot.id().is_none());
}

/// The snapshot's own keys sit beside the event's `type`, so the event
/// stream and the REST route carry one shape.
#[test]
fn a_snapshot_event_is_the_snapshot_with_a_type() {
    let snapshot = QueueSnapshot {
        revision: 4,
        max_size: 10,
        ..QueueSnapshot::default()
    };

    let event =
        serde_json::to_value(DownloadEvent::queue_snapshot(snapshot.clone())).expect("serializes");
    let mut rest = serde_json::to_value(&snapshot).expect("serializes");
    rest["type"] = "queue_snapshot".into();

    assert_eq!(event, rest);
    let back: DownloadEvent = serde_json::from_value(event).expect("parses");
    assert!(matches!(back, DownloadEvent::QueueSnapshot(s) if *s == snapshot));
}

/// Each outcome has its own event, and the event's words are the finished
/// entry's, not a second wording.
#[test]
fn an_ending_event_carries_the_finished_entrys_text() {
    let completed = ended(DownloadOutcome::Completed {
        message: Some("ok".to_string()),
    });
    let refused = ended(failed("no"));
    let cancelled = ended(DownloadOutcome::Cancelled);

    assert!(matches!(
        DownloadEvent::ended(&completed),
        DownloadEvent::DownloadCompleted { id, text }
            if id == completed.id && text == completed.text
    ));
    assert!(matches!(
        DownloadEvent::ended(&refused),
        DownloadEvent::DownloadFailed { id, text }
            if id == refused.id && text == refused.text
    ));
    assert!(matches!(
        DownloadEvent::ended(&cancelled),
        DownloadEvent::DownloadCancelled { id, text }
            if id == cancelled.id && text == cancelled.text
    ));
}
