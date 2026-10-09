//! Tests for the projector a download brings: the link, and the file rows.

use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use super::*;
use crate::domain::{Model, ModelFile, NewModel};
use crate::download::Quantization;
use crate::paths::canonical_model_path;
use crate::ports::ResolvedFile;
use crate::services::model_projector::tests::FirstBytesParser;

/// Stores what is inserted, under id 1, and finds it by the path it holds,
/// as the `SQLite` repository does by the resolved one. A link the stored
/// model has is kept when the inserted one carries none, as that upsert
/// keeps it.
#[derive(Default)]
struct OneSlotRepo(Mutex<Option<Model>>);

#[async_trait]
impl ModelRepository for OneSlotRepo {
    async fn list(&self) -> Result<Vec<Model>, RepositoryError> {
        Ok(self.0.lock().unwrap().iter().cloned().collect())
    }
    async fn get_by_id(&self, id: i64) -> Result<Model, RepositoryError> {
        Err(RepositoryError::NotFound(format!("id={id}")))
    }
    async fn get_by_name(&self, name: &str) -> Result<Model, RepositoryError> {
        Err(RepositoryError::NotFound(format!("name={name}")))
    }
    async fn find_by_path(&self, path: &Path) -> Result<Option<Model>, RepositoryError> {
        let held = self.0.lock().unwrap().clone();
        Ok(held.filter(|model| model.file_path == path))
    }
    async fn insert(&self, model: &NewModel) -> Result<Model, RepositoryError> {
        let mut stored = Model::stored(1, model);
        let held = self.0.lock().unwrap().take();
        stored.projector_path = stored
            .projector_path
            .or_else(|| held.and_then(|held| held.projector_path));
        *self.0.lock().unwrap() = Some(stored.clone());
        Ok(stored)
    }
    async fn update(&self, _model: &Model) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn delete(&self, _id: i64) -> Result<(), RepositoryError> {
        Ok(())
    }
}

/// Records each row inserted, as `(file_path, file_index, hf_oid)`.
#[derive(Default)]
struct RecordedRows(Mutex<Vec<(String, i32, Option<String>)>>);

#[async_trait]
impl ModelFilesRepositoryPort for RecordedRows {
    async fn insert(&self, file: &NewModelFile) -> Result<(), RepositoryError> {
        self.0
            .lock()
            .unwrap()
            .push((file.file_path.clone(), file.file_index, file.hf_oid.clone()));
        Ok(())
    }
    async fn get_by_model_id(&self, _model_id: i64) -> Result<Vec<ModelFile>, RepositoryError> {
        unimplemented!("a registration only stores rows")
    }
    async fn update_verification_time(
        &self,
        _id: i64,
        _at: DateTime<Utc>,
    ) -> Result<(), RepositoryError> {
        unimplemented!("a registration only stores rows")
    }
}

/// A finished download in `dir`: the weights `zeta.Q8_0.gguf`, whose name
/// sorts after the projector's, and `mmproj-F16.gguf` holding `projector`.
pub(super) fn downloaded(dir: &Path, projector: &str) -> CompletedDownload {
    let weights = dir.join("zeta.Q8_0.gguf");
    let projector_path = dir.join("mmproj-F16.gguf");
    std::fs::write(&weights, "weights").unwrap();
    std::fs::write(&projector_path, projector).unwrap();
    CompletedDownload {
        primary_path: weights.clone(),
        all_paths: vec![weights, projector_path.clone()],
        projector_path: Some(projector_path),
        components: vec![],
        quantization: Quantization::Q8_0,
        repo_id: "owner/zeta-GGUF".to_owned(),
        commit_sha: "abc123".to_owned(),
        is_sharded: false,
        file_paths: None,
        hf_tags: vec![],
        hf_file_entries: vec![
            ResolvedFile::with_size_and_oid("zeta.Q8_0.gguf", 7, Some("w-oid".to_owned())),
            ResolvedFile::projector("mmproj-F16.gguf", 9, Some("p-oid".to_owned())),
        ],
    }
}

pub(super) struct Registered {
    pub(super) answer: RegisteredDownload,
    pub(super) stored: Model,
    rows: Vec<(String, i32, Option<String>)>,
}

pub(super) async fn register(download: &CompletedDownload) -> Registered {
    register_into(Arc::new(OneSlotRepo::default()), download).await
}

async fn register_into(repo: Arc<OneSlotRepo>, download: &CompletedDownload) -> Registered {
    let rows = Arc::new(RecordedRows::default());
    let registrar =
        ModelRegistrar::new(repo.clone(), Arc::new(FirstBytesParser), Some(rows.clone()));
    let answer = registrar.register_model(download).await.unwrap();
    let stored = repo.0.lock().unwrap().clone().expect("a model is stored");
    let rows = rows.0.lock().unwrap().clone();
    Registered {
        answer,
        stored,
        rows,
    }
}

fn both_rows() -> Vec<(String, i32, Option<String>)> {
    vec![
        ("zeta.Q8_0.gguf".to_owned(), 0, Some("w-oid".to_owned())),
        ("mmproj-F16.gguf".to_owned(), 1, Some("p-oid".to_owned())),
    ]
}

#[tokio::test]
async fn the_downloaded_projector_is_linked_and_the_weights_stay_primary() {
    let dir = tempfile::tempdir().unwrap();
    let download = downloaded(dir.path(), "projector");

    let registered = register(&download).await;

    let projector = canonical_model_path(&dir.path().join("mmproj-F16.gguf")).unwrap();
    assert_eq!(registered.answer.projector_refusal, None);
    assert_eq!(
        registered.answer.model.projector_path,
        Some(projector.clone())
    );
    assert_eq!(registered.stored.projector_path, Some(projector));
    assert_eq!(registered.stored.file_path, download.primary_path);
    assert!(registered.stored.image_input());
}

#[tokio::test]
async fn every_file_of_the_group_gets_a_row_and_the_projector_is_last() {
    let dir = tempfile::tempdir().unwrap();

    let registered = register(&downloaded(dir.path(), "projector")).await;

    assert_eq!(registered.rows, both_rows());
}

/// The file named as a projector holds weights: the model is in the library,
/// it is not linked, and the answer says why.
#[tokio::test]
async fn a_projector_whose_header_is_not_a_projector_leaves_the_model_unlinked() {
    let dir = tempfile::tempdir().unwrap();
    let download = downloaded(dir.path(), "weights");

    let registered = register(&download).await;

    assert_eq!(registered.stored.projector_path, None);
    assert_eq!(registered.stored.file_path, download.primary_path);
    assert!(!registered.stored.image_input());
    let refusal = registered
        .answer
        .projector_refusal
        .expect("the refusal is reported");
    assert!(refusal.contains("mmproj-F16.gguf"), "{refusal}");
    assert!(refusal.contains("holds a model's weights"), "{refusal}");
    assert_eq!(registered.rows, both_rows(), "verification still covers it");
}

#[tokio::test]
async fn a_download_without_a_projector_is_registered_unlinked_and_reports_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mut download = downloaded(dir.path(), "projector");
    download.projector_path = None;

    let registered = register(&download).await;

    assert_eq!(registered.stored.projector_path, None);
    assert_eq!(registered.answer.projector_refusal, None);
}

/// A repair or an update downloads the group again. The model was linked by
/// hand to another file, and that link is the one it keeps.
#[tokio::test]
async fn a_model_already_linked_keeps_its_link_when_downloaded_again() {
    let dir = tempfile::tempdir().unwrap();
    let download = downloaded(dir.path(), "projector");
    let chosen = dir.path().join("elsewhere").join("mmproj-BF16.gguf");
    let mut held = NewModel::new(
        "zeta".to_owned(),
        canonical_model_path(&download.primary_path).unwrap(),
        7.0,
        chrono::Utc::now(),
    );
    held.projector_path = Some(chosen.clone());
    let repo = Arc::new(OneSlotRepo(Mutex::new(Some(Model::stored(1, &held)))));

    let registered = register_into(repo, &download).await;

    assert_eq!(registered.stored.projector_path, Some(chosen));
    let kept = registered
        .answer
        .projector_refusal
        .expect("the answer says the downloaded projector is not the link");
    assert!(kept.contains("keeps its link"), "{kept}");
    assert!(kept.contains("mmproj-BF16.gguf"), "{kept}");
    assert_eq!(registered.rows, both_rows());
}

/// The usual repair: the model is linked to the very projector the download
/// brings again. Nothing changed, and nothing is reported.
#[tokio::test]
async fn a_model_linked_to_the_downloaded_projector_reports_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let download = downloaded(dir.path(), "projector");
    let linked = canonical_model_path(&dir.path().join("mmproj-F16.gguf")).unwrap();
    let mut held = NewModel::new(
        "zeta".to_owned(),
        canonical_model_path(&download.primary_path).unwrap(),
        7.0,
        chrono::Utc::now(),
    );
    held.projector_path = Some(linked.clone());
    let repo = Arc::new(OneSlotRepo(Mutex::new(Some(Model::stored(1, &held)))));

    let registered = register_into(repo, &download).await;

    assert_eq!(registered.stored.projector_path, Some(linked));
    assert_eq!(registered.answer.projector_refusal, None);
}

/// Refuses every row.
struct RefusedRows;

#[async_trait]
impl ModelFilesRepositoryPort for RefusedRows {
    async fn insert(&self, _file: &NewModelFile) -> Result<(), RepositoryError> {
        Err(RepositoryError::Storage("the table is gone".to_owned()))
    }
    async fn get_by_model_id(&self, _model_id: i64) -> Result<Vec<ModelFile>, RepositoryError> {
        unimplemented!("a registration only stores rows")
    }
    async fn update_verification_time(
        &self,
        _id: i64,
        _at: DateTime<Utc>,
    ) -> Result<(), RepositoryError> {
        unimplemented!("a registration only stores rows")
    }
}

/// The rows are what verification reads later. A store that refuses them
/// does not lose the download: the model is in the library all the same.
#[tokio::test]
async fn a_model_whose_file_rows_are_refused_is_still_registered() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Arc::new(OneSlotRepo::default());
    let registrar = ModelRegistrar::new(
        repo.clone(),
        Arc::new(FirstBytesParser),
        Some(Arc::new(RefusedRows)),
    );

    let answer = registrar
        .register_model(&downloaded(dir.path(), "projector"))
        .await
        .expect("a refused row is not a failed registration");

    assert_eq!(answer.model.id, 1);
    assert!(repo.0.lock().unwrap().is_some(), "the model is stored");
}
