//! Tests for the download queue operations.

use std::sync::{Arc, Mutex};

use super::*;
use crate::error::GuiError;
use crate::test_support::{MockDownloadManager, MockHfClient, MockToolSupportDetector};

fn make_ops(mgr: MockDownloadManager) -> DownloadOps {
    DownloadOps::new(DownloadDeps {
        downloads: Arc::new(mgr),
        hf: Arc::new(MockHfClient),
        tool_detector: Arc::new(MockToolSupportDetector),
    })
}

/// Operations over a manager, and the calls that manager is sent.
fn recording_ops() -> (DownloadOps, Arc<Mutex<Vec<String>>>) {
    let mgr = MockDownloadManager::new();
    let calls = Arc::clone(&mgr.calls);
    (make_ops(mgr), calls)
}

#[tokio::test]
async fn get_queue_snapshot_returns_empty_snapshot() {
    let ops = make_ops(MockDownloadManager::new());
    let snapshot = ops.get_queue_snapshot().await;
    assert!(snapshot.is_idle() && snapshot.finished.is_empty());
}

/// The answer to a queue request is the ID the manager gave the download.
#[tokio::test]
async fn queue_download_answers_the_downloads_id() {
    let ops = make_ops(MockDownloadManager::new());
    let queued = ops.queue_download("mock/model".to_string(), None).await;
    assert_eq!(queued.unwrap().id, "mock/model:Q8_0");
}

#[tokio::test]
async fn cancel_download_succeeds_with_model_id_string() {
    // Ensure the parse-then-fallback path works when given a plain model ID
    let ops = make_ops(MockDownloadManager::new());
    let result = ops.cancel_download("some/model").await;
    assert!(result.is_ok());
}

/// Cancelling asks the manager to cancel that download, and nothing else:
/// a cancel must not drop an ended download's entry, as a removal does.
#[tokio::test]
async fn cancel_download_cancels_that_download() {
    let (ops, calls) = recording_ops();

    ops.cancel_download("owner/repo:Q8_0").await.unwrap();

    assert_eq!(*calls.lock().unwrap(), ["cancel_download owner/repo:Q8_0"]);
}

#[tokio::test]
async fn cancel_download_not_found_maps_to_gui_error() {
    let ops = make_ops(MockDownloadManager::failing_cancel());
    let result = ops.cancel_download("some/model").await;
    assert!(
        matches!(
            result,
            Err(GuiError::NotFound {
                entity: "download",
                ..
            })
        ),
        "expected GuiError::NotFound, got {result:?}"
    );
}

/// Removing asks the manager to take that download off the queue, which
/// also drops an ended download's entry; a cancel would leave it.
#[tokio::test]
async fn remove_from_queue_removes_that_download() {
    let (ops, calls) = recording_ops();

    ops.remove_from_queue("owner/repo:Q8_0").await.unwrap();

    assert_eq!(
        *calls.lock().unwrap(),
        ["remove_from_queue owner/repo:Q8_0"]
    );
}

#[tokio::test]
async fn reorder_queue_returns_new_position() {
    let mgr = MockDownloadManager {
        reorder_position: 3,
        ..MockDownloadManager::default()
    };
    let ops = make_ops(mgr);
    let result = ops.reorder_queue("some/model", 3).await;
    assert_eq!(result.unwrap(), 3);
}

/// Clearing reaches the manager. It answers nothing, so the call is all
/// there is to see.
#[tokio::test]
async fn clear_finished_clears_the_managers_record() {
    let (ops, calls) = recording_ops();

    ops.clear_finished().await;

    assert_eq!(*calls.lock().unwrap(), ["clear_finished"]);
}

#[tokio::test]
async fn cancel_all_completes_without_error() {
    let ops = make_ops(MockDownloadManager::new());
    // cancel_all is fire-and-forget (returns ())
    ops.cancel_all().await;
}

/// On the wire the answer to a queue request is the ID alone, under `id`:
/// the daemon writes this type and the CLI reads it.
#[test]
fn the_queue_answer_is_the_id_alone() {
    let answer = QueueDownloadResponse {
        id: "owner/repo:Q8_0".to_string(),
    };

    let json = serde_json::to_value(&answer).unwrap();

    assert_eq!(json, serde_json::json!({ "id": "owner/repo:Q8_0" }));
    let back: QueueDownloadResponse = serde_json::from_value(json).unwrap();
    assert_eq!(back, answer);
}
