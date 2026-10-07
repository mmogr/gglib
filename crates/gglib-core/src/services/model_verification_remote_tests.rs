//! Tests for the update check and repair of a model that has a projector.

use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};

use super::*;
use crate::domain::{Model, NewModel, NewModelFile};
use crate::ports::ModelFilesRepositoryPort;
use crate::ports::huggingface::fake_hub::{FakeHub, hub_file};
use crate::services::DownloadTriggerPort;
use crate::services::model_projector::tests::OneModelRepo;

const REPO: &str = "owner/zeta-GGUF";
pub(super) const WEIGHTS: &str = "zeta.Q8_0.gguf";
pub(super) const PROJECTOR: &str = "mmproj-F16.gguf";

/// Answers the rows it was made with.
pub(crate) struct Rows(pub(crate) Vec<ModelFile>);

#[async_trait]
impl ModelFilesRepositoryPort for Rows {
    async fn insert(&self, _file: &NewModelFile) -> Result<(), RepositoryError> {
        unimplemented!("the rows are the ones it was made with")
    }
    async fn get_by_model_id(&self, _model_id: i64) -> Result<Vec<ModelFile>, RepositoryError> {
        Ok(self.0.clone())
    }
    async fn update_verification_time(
        &self,
        _id: i64,
        _at: DateTime<Utc>,
    ) -> Result<(), RepositoryError> {
        Ok(())
    }
}

/// Records each download asked for, as `(repo, quantization)`.
#[derive(Default)]
pub(super) struct Queued(pub Mutex<Vec<(String, Option<String>)>>);

#[async_trait]
impl DownloadTriggerPort for Queued {
    async fn queue_download(
        &self,
        repo_id: String,
        quantization: Option<String>,
    ) -> anyhow::Result<String> {
        self.0.lock().unwrap().push((repo_id, quantization));
        Ok("queued".to_owned())
    }
}

pub(super) fn sha256(content: &str) -> String {
    format!("{:x}", Sha256::digest(content.as_bytes()))
}

pub(super) fn row(index: i32, name: &str, oid: &str) -> ModelFile {
    ModelFile {
        id: i64::from(index) + 1,
        model_id: 1,
        file_path: name.to_owned(),
        file_index: index,
        expected_size: 1,
        hf_oid: Some(oid.to_owned()),
        last_verified_at: None,
    }
}

/// Model 1, a `Q8_0` download of [`REPO`] whose weights are in `dir`.
fn model_in(dir: &Path) -> Model {
    let mut new = NewModel::new("zeta".to_owned(), dir.join(WEIGHTS), 7.0, Utc::now());
    new.hf_repo_id = Some(REPO.to_owned());
    new.quantization = Some("Q8_0".to_owned());
    Model::stored(1, &new)
}

pub(super) struct Fixture {
    pub service: ModelVerificationService,
    pub hub: Arc<FakeHub>,
    pub queued: Arc<Queued>,
}

pub(super) fn fixture(dir: &Path, rows: Vec<ModelFile>, hub: FakeHub) -> Fixture {
    fixture_over(dir, Arc::new(Rows(rows)), hub)
}

/// [`fixture`] over a store of the caller's.
pub(super) fn fixture_over(
    dir: &Path,
    store: Arc<dyn ModelFilesRepositoryPort>,
    hub: FakeHub,
) -> Fixture {
    let hub = Arc::new(hub);
    let queued = Arc::new(Queued::default());
    let service = ModelVerificationService::new(
        Arc::new(OneModelRepo(Mutex::new(model_in(dir)))),
        store,
        hub.clone(),
        queued.clone(),
    );
    Fixture {
        service,
        hub,
        queued,
    }
}

// ── The update check ─────────────────────────────────────────────────────

#[tokio::test]
async fn the_update_check_reports_a_changed_projector_oid() {
    let rows = vec![row(0, WEIGHTS, "w-oid"), row(1, PROJECTOR, "p-old")];
    let hub = FakeHub {
        weights: vec![hub_file(WEIGHTS, 1, "w-oid")],
        projectors: vec![
            hub_file("mmproj-BF16.gguf", 1, "other"),
            hub_file(PROJECTOR, 1, "p-new"),
        ],
        ..Default::default()
    };
    let f = fixture(Path::new("/models/zeta"), rows, hub);

    let result = f.service.check_for_updates(1).await.unwrap();

    assert!(result.update_available);
    let details = result.details.expect("the change is described");
    assert_eq!(details.changed_shards, 1);
    let change = &details.changes[0];
    assert_eq!(change.file_path, PROJECTOR);
    assert_eq!(change.index, 1);
    assert_eq!(change.old_oid, "p-old");
    assert_eq!(change.new_oid, "p-new");
}

#[tokio::test]
async fn an_unchanged_projector_is_no_update() {
    let rows = vec![row(0, WEIGHTS, "w-oid"), row(1, PROJECTOR, "p-oid")];
    let hub = FakeHub {
        weights: vec![hub_file(WEIGHTS, 1, "w-oid")],
        projectors: vec![hub_file(PROJECTOR, 1, "p-oid")],
        ..Default::default()
    };
    let f = fixture(Path::new("/models/zeta"), rows, hub);

    let result = f.service.check_for_updates(1).await.unwrap();

    assert!(!result.update_available);
    assert!(result.details.is_none());
    assert_eq!(f.hub.projector_listings.load(Ordering::Relaxed), 1);
}

/// A model downloaded without a projector has no row for one, and the
/// repository's projectors are not listed for it.
#[tokio::test]
async fn a_model_without_a_projector_row_checks_its_weights_alone() {
    let rows = vec![row(0, WEIGHTS, "w-old")];
    let hub = FakeHub {
        weights: vec![hub_file(WEIGHTS, 1, "w-new")],
        projectors: vec![hub_file(PROJECTOR, 1, "p-new")],
        ..Default::default()
    };
    let f = fixture(Path::new("/models/zeta"), rows, hub);

    let result = f.service.check_for_updates(1).await.unwrap();

    let details = result.details.expect("the weights changed");
    assert_eq!(details.changed_shards, 1);
    assert_eq!(details.changes[0].file_path, WEIGHTS);
    assert_eq!(f.hub.projector_listings.load(Ordering::Relaxed), 0);
}

// ── Repair ───────────────────────────────────────────────────────────────

/// A directory holding healthy weights and a projector whose bytes no longer
/// match its row, and the rows for both.
fn corrupt_projector_on_disk(dir: &Path) -> Vec<ModelFile> {
    std::fs::write(dir.join(WEIGHTS), "weights").unwrap();
    std::fs::write(dir.join(PROJECTOR), "damaged").unwrap();
    vec![
        row(0, WEIGHTS, &sha256("weights")),
        row(1, PROJECTOR, &sha256("projector")),
    ]
}

#[tokio::test]
async fn a_corrupt_projector_is_deleted_and_its_group_is_queued_again() {
    let dir = tempfile::tempdir().unwrap();
    let rows = corrupt_projector_on_disk(dir.path());
    let hub = FakeHub {
        weights: vec![hub_file(WEIGHTS, 7, "w-oid")],
        projectors: vec![hub_file(PROJECTOR, 9, "p-oid")],
        ..Default::default()
    };
    let f = fixture(dir.path(), rows, hub);

    let queued_as = f.service.repair_model(1, None).await.unwrap();

    assert_eq!(queued_as, "queued");
    assert!(
        !dir.path().join(PROJECTOR).exists(),
        "the corrupt file goes"
    );
    assert!(dir.path().join(WEIGHTS).exists(), "healthy weights stay");
    assert_eq!(
        *f.queued.0.lock().unwrap(),
        [(REPO.to_owned(), Some("Q8_0".to_owned()))]
    );
    // What was queued is a group that holds the projector.
    let group = download_group(f.hub.as_ref(), REPO, Quantization::Q8_0)
        .await
        .unwrap();
    assert!(group.files().any(|file| file.path == PROJECTOR));
}

/// The repository no longer has the projector the model was downloaded with:
/// a download would not bring it back, so it is not deleted.
#[tokio::test]
async fn a_corrupt_projector_no_download_fetches_is_left_in_place() {
    let dir = tempfile::tempdir().unwrap();
    let rows = corrupt_projector_on_disk(dir.path());
    let hub = FakeHub {
        weights: vec![hub_file(WEIGHTS, 7, "w-oid")],
        projectors: vec![hub_file("mmproj-BF16.gguf", 9, "other")],
        ..Default::default()
    };
    let f = fixture(dir.path(), rows, hub);

    let refused = f.service.repair_model(1, None).await.unwrap_err();

    assert!(refused.contains(PROJECTOR), "{refused}");
    assert!(refused.contains("--projector"), "{refused}");
    assert!(dir.path().join(PROJECTOR).exists(), "nothing is deleted");
    assert!(f.queued.0.lock().unwrap().is_empty(), "nothing is queued");
}

/// The same projector beside corrupt weights: the weights are repaired, and
/// the projector the download would not bring back still stays.
#[tokio::test]
async fn corrupt_weights_are_repaired_around_a_projector_that_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let rows = corrupt_projector_on_disk(dir.path());
    std::fs::write(dir.path().join(WEIGHTS), "damaged").unwrap();
    let hub = FakeHub {
        weights: vec![hub_file(WEIGHTS, 7, "w-oid")],
        ..Default::default()
    };
    let f = fixture(dir.path(), rows, hub);

    f.service.repair_model(1, None).await.unwrap();

    assert!(!dir.path().join(WEIGHTS).exists());
    assert!(dir.path().join(PROJECTOR).exists());
    assert_eq!(f.queued.0.lock().unwrap().len(), 1);
}

/// Repair by index, as the update flow asks it: the projector's row index.
#[tokio::test]
async fn a_projector_named_by_its_index_is_repaired() {
    let dir = tempfile::tempdir().unwrap();
    let rows = corrupt_projector_on_disk(dir.path());
    let hub = FakeHub {
        weights: vec![hub_file(WEIGHTS, 7, "w-oid")],
        projectors: vec![hub_file(PROJECTOR, 9, "p-new")],
        ..Default::default()
    };
    let f = fixture(dir.path(), rows, hub);

    f.service.repair_model(1, Some(vec![1])).await.unwrap();

    assert!(!dir.path().join(PROJECTOR).exists());
    assert!(dir.path().join(WEIGHTS).exists());
    assert_eq!(f.queued.0.lock().unwrap().len(), 1);
}
