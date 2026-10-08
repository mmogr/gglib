//! Tests for a `model_files` store that fails: a read that fails stops the
//! operation with the store's words, which a verification prints alone and a
//! repair under their label, and a verification time that cannot be written
//! costs the report nothing.

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use super::tests::{WEIGHTS, fixture_over, row, sha256};
use crate::domain::{ModelFile, NewModelFile};
use crate::ports::huggingface::fake_hub::FakeHub;
use crate::ports::{ModelFilesRepositoryPort, RepositoryError};
use crate::services::OverallHealth;

/// What the store says when it fails.
const WHY: &str = "the table is gone";

/// Answers the rows it holds, fails the read when it holds none, and fails
/// every write, each time with [`WHY`] as an error of the kind it was made
/// with.
struct Failing(Option<Vec<ModelFile>>, fn(String) -> RepositoryError);

impl Failing {
    fn gone(&self) -> RepositoryError {
        (self.1)(WHY.to_owned())
    }
}

/// A store whose table cannot be read or written.
fn unreadable() -> Arc<Failing> {
    Arc::new(Failing(None, RepositoryError::Storage))
}

#[async_trait]
impl ModelFilesRepositoryPort for Failing {
    async fn insert(&self, _file: &NewModelFile) -> Result<(), RepositoryError> {
        Err(self.gone())
    }
    async fn get_by_model_id(&self, _model_id: i64) -> Result<Vec<ModelFile>, RepositoryError> {
        self.0.clone().ok_or_else(|| self.gone())
    }
    async fn update_verification_time(
        &self,
        _id: i64,
        _at: DateTime<Utc>,
    ) -> Result<(), RepositoryError> {
        Err(self.gone())
    }
}

#[tokio::test]
async fn an_unreadable_store_fails_the_update_check_as_a_storage_error_in_its_words() {
    let dir = Path::new("/models/zeta");
    let f = fixture_over(dir, unreadable(), FakeHub::default());

    let failed = f.service.check_for_updates(1).await.unwrap_err();

    assert!(
        matches!(&failed, RepositoryError::Storage(why) if why == WHY),
        "{failed:?}"
    );
}

#[tokio::test]
async fn an_unreadable_store_stops_a_verification_before_it_starts() {
    let dir = Path::new("/models/zeta");
    let f = fixture_over(dir, unreadable(), FakeHub::default());

    let failed = f.service.verify_model_integrity(1).await.unwrap_err();

    assert_eq!(failed, format!("Failed to get model files: {WHY}"));
}

#[tokio::test]
async fn a_read_that_fails_another_way_stops_a_verification_under_its_label() {
    let dir = Path::new("/models/zeta");
    let store = Arc::new(Failing(None, RepositoryError::Serialization));
    let f = fixture_over(dir, store, FakeHub::default());

    let failed = f.service.verify_model_integrity(1).await.unwrap_err();

    assert_eq!(
        failed,
        format!("Failed to get model files: Serialization error: {WHY}")
    );
}

#[tokio::test]
async fn an_unreadable_store_stops_a_repair_before_anything_is_deleted() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(WEIGHTS), "damaged").unwrap();
    let f = fixture_over(dir.path(), unreadable(), FakeHub::default());

    let failed = f.service.repair_model(1, None).await.unwrap_err();

    assert_eq!(
        failed,
        format!("Failed to get model files: Storage error: {WHY}")
    );
    assert!(dir.path().join(WEIGHTS).exists(), "nothing is deleted");
    assert!(f.queued.asked().is_empty(), "nothing is queued");
}

#[tokio::test]
async fn a_verification_time_that_cannot_be_written_still_answers_the_report() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(WEIGHTS), "weights").unwrap();
    let rows = vec![row(0, WEIGHTS, &sha256("weights"))];
    let store = Arc::new(Failing(Some(rows), RepositoryError::Storage));
    let f = fixture_over(dir.path(), store, FakeHub::default());

    let (mut progress, verifying) = f.service.verify_model_integrity(1).await.unwrap();
    while progress.recv().await.is_some() {}
    let report = verifying.await.unwrap().unwrap();

    assert_eq!(report.overall_health, OverallHealth::Healthy);
}
