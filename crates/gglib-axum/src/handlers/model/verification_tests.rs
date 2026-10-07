//! What the three verification routes answer: for an id no model has, for a
//! model that did not come from Hugging Face, and for one that did.

use std::sync::Arc;

use axum::http::StatusCode;
use axum::response::IntoResponse;
use gglib_core::domain::{NewModel, NewModelFile};
use gglib_core::download::DownloadError;
use gglib_core::ports::huggingface::fake_hub::{FakeHub, hub_file};
use gglib_core::ports::{AskedDownloads, ModelFilesRepositoryPort};
use gglib_core::services::AppCore;
use http_body_util::BodyExt;
use serde_json::{Value, json};

use super::*;
use crate::handlers::agent::run_fixture::state;

/// An id no model in a new library has.
const UNKNOWN: i64 = 999_999;
const REPO: &str = "owner/zeta-GGUF";
const WEIGHTS: &str = "zeta.Q8_0.gguf";
/// The SHA-256 of `weights`, the bytes of a healthy [`WEIGHTS`].
const HEALTHY_OID: &str = "9a129038d9a00aed0cf6a7ea059ca50a813449061ab87848cf1a13eafdf33b2c";
/// What [`WEIGHTS`] is on the Hub now.
const HUB_OID: &str = "newer";
/// The id the queue gives a `Q8_0` download of [`REPO`].
const QUEUED: &str = "owner/zeta-GGUF:Q8_0";

/// A Hub whose every quantization is [`WEIGHTS`] under [`HUB_OID`].
fn hub() -> FakeHub {
    FakeHub {
        weights: vec![hub_file(WEIGHTS, 7, HUB_OID)],
        ..FakeHub::default()
    }
}

/// A daemon whose library is in `dir`, with [`hub`] and `queue` behind its
/// verification service.
struct Library {
    state: AppState,
    files: Arc<dyn ModelFilesRepositoryPort>,
    queue: Arc<AskedDownloads>,
    dir: tempfile::TempDir,
}

async fn library() -> Library {
    library_queueing_on(AskedDownloads::default()).await
}

/// [`library`], whose repairs queue on `queue`.
async fn library_queueing_on(queue: AskedDownloads) -> Library {
    let (dir, state) = state().await;
    let url = format!("sqlite:{}", dir.path().join("gglib.db").display());
    let pool = sqlx::SqlitePool::connect(&url).await.unwrap();
    let repos = gglib_db::CoreFactory::build_repos(pool);
    let files = Arc::clone(&repos.model_files);
    let queue = Arc::new(queue);
    let mut state = Arc::into_inner(state).expect("the only holder");
    state.core = Arc::new(AppCore::new(repos, Arc::new(hub()), queue.clone()));
    Library {
        state: Arc::new(state),
        files,
        queue,
        dir,
    }
}

impl Library {
    /// Add a model a user pointed gglib at: no repository, no file rows.
    async fn local_model(&self) -> i64 {
        let path = self.dir.path().join("local.gguf");
        let model = NewModel::new("local".to_owned(), path, 7.0, chrono::Utc::now());
        self.state.core.models().add(model).await.unwrap().id
    }

    /// Add a `Q8_0` download of [`REPO`] whose weights hold `bytes`, with the
    /// row a healthy file is verified against.
    async fn downloaded_model(&self, bytes: &str) -> i64 {
        let path = self.dir.path().join(WEIGHTS);
        std::fs::write(&path, bytes).unwrap();
        let mut model = NewModel::new("zeta".to_owned(), path, 7.0, chrono::Utc::now());
        model.hf_repo_id = Some(REPO.to_owned());
        model.quantization = Some("Q8_0".to_owned());
        let id = self.state.core.models().add(model).await.unwrap().id;
        let oid = Some(HEALTHY_OID.to_owned());
        let row = NewModelFile::new(id, WEIGHTS.to_owned(), 0, 7, oid);
        self.files.insert(&row).await.unwrap();
        id
    }

    async fn verify(&self, id: i64) -> (StatusCode, Value) {
        answer(verify(State(self.state.clone()), Path(id)).await).await
    }

    async fn check_updates(&self, id: i64) -> (StatusCode, Value) {
        answer(check_updates(State(self.state.clone()), Path(id)).await).await
    }

    async fn repair(&self, id: i64) -> (StatusCode, Value) {
        let all = Json(RepairRequest { shards: None });
        answer(repair(State(self.state.clone()), Path(id), all).await).await
    }
}

/// The status and JSON body a handler's result goes out as.
async fn answer(result: impl IntoResponse) -> (StatusCode, Value) {
    let response = result.into_response();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).expect("a JSON body"))
}

#[tokio::test]
async fn an_unknown_model_is_not_found_by_verify_check_updates_and_repair() {
    let library = library().await;
    let not_found = (
        StatusCode::NOT_FOUND,
        json!({ "error": "Model with ID 999999 not found", "status": 404 }),
    );

    assert_eq!(library.verify(UNKNOWN).await, not_found);
    assert_eq!(library.check_updates(UNKNOWN).await, not_found);
    assert_eq!(library.repair(UNKNOWN).await, not_found);
    assert!(library.queue.asked().is_empty());
}

#[tokio::test]
async fn a_model_with_no_hugging_face_source_has_nothing_to_verify_or_repair_and_is_up_to_date() {
    let library = library().await;
    let id = library.local_model().await;

    let (status, body) = library.verify(id).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    let error = "Failed to start verification: No model files found for verification";
    assert_eq!(body, json!({ "error": error, "status": 500 }));

    let (status, body) = library.check_updates(id).await;
    assert_eq!(status, StatusCode::OK);
    let result = json!({ "model_id": id, "update_available": false, "details": null });
    assert_eq!(
        body,
        json!({ "result": result, "message": "Model is up to date" })
    );

    let (status, body) = library.repair(id).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    let error = "Failed to repair model: Model does not have HuggingFace repository information";
    assert_eq!(body, json!({ "error": error, "status": 500 }));
    assert!(library.queue.asked().is_empty());
}

#[tokio::test]
async fn verify_answers_the_report_of_a_healthy_download() {
    let library = library().await;
    let id = library.downloaded_model("weights").await;

    let (status, mut body) = library.verify(id).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    let verified_at = body["report"]["verified_at"].take();
    assert!(verified_at.is_string(), "{verified_at}");
    let shard = json!({ "index": 0, "file_path": WEIGHTS, "health": { "type": "healthy" } });
    let report = json!({
        "model_id": id,
        "overall_health": "healthy",
        "shards": [shard],
        "verified_at": null,
    });
    assert_eq!(body, json!({ "report": report }));
}

#[tokio::test]
async fn check_updates_answers_the_file_the_hub_has_changed() {
    let library = library().await;
    let id = library.downloaded_model("weights").await;

    let (status, body) = library.check_updates(id).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    let change = json!({
        "index": 0,
        "file_path": WEIGHTS,
        "old_oid": HEALTHY_OID,
        "new_oid": HUB_OID,
    });
    let details = json!({ "changed_shards": 1, "changes": [change] });
    let result = json!({ "model_id": id, "update_available": true, "details": details });
    let message = "Updates available: 1 shards can be updated";
    assert_eq!(body, json!({ "result": result, "message": message }));
}

#[tokio::test]
async fn repair_deletes_a_corrupt_file_and_answers_the_download_it_queued() {
    let library = library().await;
    let id = library.downloaded_model("damaged").await;

    let (status, body) = library.repair(id).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body, json!({ "id": QUEUED, "files": [WEIGHTS] }));
    assert!(!library.dir.path().join(WEIGHTS).exists(), "the file goes");
    assert_eq!(
        library.queue.asked(),
        [(REPO.to_owned(), Some("Q8_0".to_owned()))]
    );
}

/// The queue refuses once the file is gone. The route fails, and its error
/// says which file is missing and the command that fetches it.
#[tokio::test]
async fn a_repair_whose_download_cannot_be_queued_fails_and_names_the_missing_file() {
    let full = AskedDownloads::refusing(DownloadError::queue_full(10));
    let library = library_queueing_on(full).await;
    let id = library.downloaded_model("damaged").await;

    let (status, body) = library.repair(id).await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    let error = "Failed to repair model: The download that fetches them again could not be \
                 queued: Queue full: maximum 10 downloads allowed. Missing from the model's \
                 folder: zeta.Q8_0.gguf. Run `gglib model download owner/zeta-GGUF \
                 --quantization Q8_0` to fetch what is missing.";
    assert_eq!(body, json!({ "error": error, "status": 500 }));
    assert!(!library.dir.path().join(WEIGHTS).exists(), "as it says");
}
