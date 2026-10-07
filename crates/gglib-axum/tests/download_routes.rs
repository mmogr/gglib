//! The download queue's routes, each to the manager call it makes: queueing
//! answers the id the manager gave, cancel cancels and DELETE removes.

mod common;

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::http::{Method, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

use common::harness::{test_access, test_state};
use common::origin::authed;
use gglib_app_services::{DownloadDeps, DownloadOps};
use gglib_core::CorsConfig;
use gglib_core::download::{DownloadError, DownloadId, QueueSnapshot};
use gglib_core::ports::DownloadManagerPort;
use gglib_gguf::ToolSupportDetector;

/// The id the manager answers a queue request with: a quantization chosen
/// for a request that named none.
const QUEUED: &str = "owner/mine:Q8_0";
/// `QUEUED` as one path segment.
const QUEUED_IN_A_PATH: &str = "owner%2Fmine:Q8_0";

/// A manager that keeps the calls it is sent. It holds no download, so a
/// cancel and a removal both answer that theirs is not in the queue.
#[derive(Default)]
struct Recording(Mutex<Vec<String>>);

impl Recording {
    fn record(&self, call: String) {
        self.0.lock().unwrap().push(call);
    }

    fn calls(&self) -> Vec<String> {
        self.0.lock().unwrap().clone()
    }
}

#[async_trait]
impl DownloadManagerPort for Recording {
    async fn queue_smart(
        self: Arc<Self>,
        repo_id: String,
        quantization: Option<String>,
    ) -> Result<DownloadId, DownloadError> {
        self.record(format!("queue_smart {repo_id} {quantization:?}"));
        Ok(DownloadId::from(QUEUED))
    }

    async fn get_queue_snapshot(&self) -> Result<QueueSnapshot, DownloadError> {
        Ok(QueueSnapshot::default())
    }

    async fn cancel_download(&self, id: &DownloadId) -> Result<(), DownloadError> {
        self.record(format!("cancel_download {id}"));
        Err(DownloadError::not_in_queue(id.to_string()))
    }

    async fn cancel_all(&self) -> Result<(), DownloadError> {
        self.record("cancel_all".to_string());
        Ok(())
    }

    async fn active_count(&self) -> Result<u32, DownloadError> {
        Ok(0)
    }

    async fn remove_from_queue(&self, id: &DownloadId) -> Result<(), DownloadError> {
        self.record(format!("remove_from_queue {id}"));
        Err(DownloadError::not_in_queue(id.to_string()))
    }

    async fn reorder_queue(&self, id: &DownloadId, position: u32) -> Result<u32, DownloadError> {
        self.record(format!("reorder_queue {id} {position}"));
        Ok(position)
    }

    async fn set_max_queue_size(&self, size: u32) -> Result<(), DownloadError> {
        self.record(format!("set_max_queue_size {size}"));
        Ok(())
    }
}

/// The daemon's router with `manager` behind its download routes.
async fn app_over(manager: Arc<Recording>) -> Router {
    let cors = CorsConfig::AllowAll;
    let mut state = Arc::into_inner(test_state().await).expect("the only holder");
    state.downloads = Arc::new(DownloadOps::new(DownloadDeps {
        downloads: manager,
        hf: Arc::clone(&state.hf_client),
        tool_detector: Arc::new(ToolSupportDetector::new()),
    }));
    gglib_axum::create_router(Arc::new(state), &cors, test_access())
}

/// Send `method` to `path` with `body` as JSON, and answer the status and
/// the body.
async fn send(app: Router, method: Method, path: &str, body: &'static str) -> (StatusCode, String) {
    let request = authed()
        .method(method)
        .uri(path)
        .header("host", "127.0.0.1:9887")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

/// The answer to a queue request is the id the manager gave the download,
/// which is not the one the request spelled: it named no quantization.
#[tokio::test]
async fn queueing_answers_the_id_the_manager_gave() {
    let manager = Arc::new(Recording::default());
    let app = app_over(Arc::clone(&manager)).await;

    let path = "/api/models/downloads/queue";
    let (status, body) = send(app, Method::POST, path, r#"{"model_id":"owner/mine"}"#).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    let json: serde_json::Value = serde_json::from_str(&body).expect("a JSON body");
    assert_eq!(json, serde_json::json!({ "id": QUEUED }));
    assert_eq!(manager.calls(), ["queue_smart owner/mine None"]);
}

/// A cancel reaches the manager's cancel, and one for a download that is no
/// longer in the queue still succeeds: it ended while the request was on
/// its way, which is what was asked for.
#[tokio::test]
async fn a_cancel_cancels_and_succeeds_for_a_download_that_is_gone() {
    let manager = Arc::new(Recording::default());
    let app = app_over(Arc::clone(&manager)).await;

    let path = format!("/api/models/downloads/{QUEUED_IN_A_PATH}/cancel");
    let (status, body) = send(app, Method::POST, &path, "").await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(manager.calls(), [format!("cancel_download {QUEUED}")]);
}

/// A DELETE reaches the manager's removal, not its cancel, and one for a
/// download the queue does not know is not found.
#[tokio::test]
async fn a_delete_removes_and_is_not_found_for_a_download_that_is_gone() {
    let manager = Arc::new(Recording::default());
    let app = app_over(Arc::clone(&manager)).await;

    let path = format!("/api/models/downloads/{QUEUED_IN_A_PATH}");
    let (status, body) = send(app, Method::DELETE, &path, "").await;

    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(manager.calls(), [format!("remove_from_queue {QUEUED}")]);
}
