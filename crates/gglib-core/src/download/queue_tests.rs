//! The wire shape of the queue snapshot.

use super::*;
use crate::download::{DownloadId, RowFacts, row};

fn waiting_row() -> DownloadRow {
    let id = DownloadId::new("owner/repo", Some("Q8_0"));
    row(&RowFacts::waiting(&id, 1, None, None))
}

/// A key that has no value is left out, and is never `null`.
#[test]
fn an_absent_value_is_an_absent_key() {
    let snapshot = QueueSnapshot {
        waiting: vec![waiting_row()],
        ..QueueSnapshot::default()
    };

    let json = serde_json::to_value(&snapshot).expect("serializes");

    assert!(json.get("active").is_none(), "{json}");
    let row = &json["waiting"][0];
    for key in ["total_bytes", "percent", "speed_bps", "eta_seconds"] {
        assert!(row.get(key).is_none(), "{key} in {row}");
    }
    assert!(row["text"].get("file").is_none(), "{row}");
    assert_eq!(row["phase"], "queued");
    assert_eq!(row["quantization"], "Q8_0");
}

#[test]
fn an_outcome_is_tagged_by_kind() {
    let outcomes = [
        (
            DownloadOutcome::Completed { message: None },
            serde_json::json!({ "kind": "completed" }),
        ),
        (
            DownloadOutcome::Failed {
                error: "no route".to_string(),
            },
            serde_json::json!({ "kind": "failed", "error": "no route" }),
        ),
        (
            DownloadOutcome::Cancelled,
            serde_json::json!({ "kind": "cancelled" }),
        ),
    ];

    for (outcome, wire) in outcomes {
        assert_eq!(serde_json::to_value(&outcome).expect("serializes"), wire);
        let back: DownloadOutcome = serde_json::from_value(wire).expect("parses");
        assert_eq!(back, outcome);
    }
}

#[test]
fn a_snapshot_survives_the_wire() {
    let snapshot = QueueSnapshot {
        revision: 7,
        active: Some(waiting_row()),
        waiting: vec![waiting_row()],
        finished: vec![FinishedDownload::new(
            &DownloadId::new("a/b", Some("Q4_K_M")),
            DownloadOutcome::Cancelled,
        )],
        max_size: 10,
        full: false,
    };

    let json = serde_json::to_string(&snapshot).expect("serializes");
    let back: QueueSnapshot = serde_json::from_str(&json).expect("parses");

    assert_eq!(back, snapshot);
    assert_eq!(back.rows().count(), 2);
    assert!(!back.is_idle());
    assert!(QueueSnapshot::default().is_idle());
}

/// How a download ended is worded here, once: the download's name and then
/// what became of it. A completed download says what its message does.
#[test]
fn a_finished_download_says_how_it_ended() {
    let id = DownloadId::new("owner/repo", Some("Q8_0"));
    let text = |outcome| FinishedDownload::new(&id, outcome).text;

    let ended = FinishedDownload::new(&id, DownloadOutcome::Cancelled);
    assert_eq!(ended.id, "owner/repo:Q8_0");
    assert_eq!(ended.title, "owner/repo:Q8_0");
    assert_eq!(ended.text, "owner/repo:Q8_0: download cancelled");
    assert_eq!(
        text(DownloadOutcome::Completed { message: None }),
        "owner/repo:Q8_0: downloaded"
    );
    assert_eq!(
        text(DownloadOutcome::Completed {
            message: Some("Downloaded model to m.gguf".to_string()),
        }),
        "owner/repo:Q8_0: Downloaded model to m.gguf"
    );
    assert_eq!(
        text(DownloadOutcome::Failed {
            error: "no route".to_string(),
        }),
        "owner/repo:Q8_0: download failed: no route"
    );
}
