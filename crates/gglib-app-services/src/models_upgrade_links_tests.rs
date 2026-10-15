//! Tests that an upgrade records its download on the model it upgrades: the
//! projector it brought is linked by the registrar's rule, each file's row
//! takes its new OID, and the model keeps its row and its owner's fields.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gglib_core::domain::{InferenceConfig, NewModel, NewModelFile};
use gglib_core::ports::{ModelFilesRepositoryPort, NoopEmitter, NoopModelRuntime, ResolvedFile};
use gglib_core::services::AppCore;
use tempfile::TempDir;

use super::*;
use crate::models::ModelDeps;
use crate::models_projector::tests::FirstBytesParser;

const REPO: &str = "owner/zeta-GGUF";
const RECORDED: &str = "1111111111111111111111111111111111111111";
const NEWER: &str = "2222222222222222222222222222222222222222";
const WEIGHTS: &str = "zeta.Q8_0.gguf";
const PROJECTOR: &str = "mmproj-F16.gguf";

/// One model of [`REPO`] at [`RECORDED`] whose weights are on disk in `dir`,
/// with a `model_files` row holding the old OID.
struct Library {
    core: Arc<AppCore>,
    ops: ModelOps,
    id: i64,
    dir: TempDir,
}

/// Writes `name` in `dir` holding `bytes`, and answers its canonical path.
fn file(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, bytes).unwrap();
    path.canonicalize().unwrap()
}

/// The library, its model linked to the projector named `linked` in its
/// folder, or to none.
async fn library(linked: Option<&str>) -> Library {
    gglib_core::paths::isolate_data_root();
    let pool = gglib_db::setup_test_database().await.expect("a database");
    let repos = gglib_db::CoreFactory::build_repos(pool);
    let rows: Arc<dyn ModelFilesRepositoryPort> = Arc::clone(&repos.model_files);
    let core = Arc::new(AppCore::bare(repos));
    let dir = tempfile::tempdir().unwrap();

    let weights = file(dir.path(), WEIGHTS, b"weights");
    let mut model = NewModel::new("zeta".to_string(), weights, 8.0, chrono::Utc::now());
    model.hf_repo_id = Some(REPO.to_string());
    model.hf_commit_sha = Some(RECORDED.to_string());
    model.quantization = Some("Q8_0".to_string());
    model.tags = vec!["mine".to_string()];
    model.inference_defaults = Some(InferenceConfig {
        temperature: Some(0.3),
        ..Default::default()
    });
    model.projector_path = linked.map(|name| file(dir.path(), name, b"projector"));
    let id = core.models().add(model).await.expect("a model").id;
    let row = NewModelFile::new(id, WEIGHTS.to_string(), 0, 7, Some("old-oid".to_string()));
    rows.insert(&row).await.expect("the row");

    let ops = ModelOps::new(ModelDeps {
        core: Arc::clone(&core),
        runtime: Arc::new(NoopModelRuntime),
        gguf_parser: Arc::new(FirstBytesParser),
        emitter: Arc::new(NoopEmitter::new()),
    });
    Library { core, ops, id, dir }
}

/// A check that finds [`NEWER`].
fn newer(
    _: String,
    recorded: Option<String>,
    _: Option<String>,
) -> std::future::Ready<anyhow::Result<UpdateCheckResult>> {
    std::future::ready(Ok(UpdateCheckResult {
        has_update: true,
        current_sha: recorded,
        latest_sha: NEWER.to_string(),
    }))
}

/// What a forced download of the model at [`NEWER`] answers: the weights
/// again, with a new OID, and [`PROJECTOR`], written in `dir`.
fn fetched(dir: &Path, request: &CliUpdateRequest) -> CliDownloadResult {
    let projector = file(dir, PROJECTOR, b"projector");
    CliDownloadResult {
        downloaded_paths: vec![request.model_path.clone(), projector],
        primary_path: request.model_path.clone(),
        quantization: request.quantization.clone(),
        repo_id: request.repo_id.clone(),
        commit_sha: NEWER.to_string(),
        files: vec![
            ResolvedFile::with_size_and_oid(WEIGHTS, 7, Some("new-oid".to_string())),
            ResolvedFile::projector(PROJECTOR, 9, Some("p-oid".to_string())),
        ],
    }
}

impl Library {
    /// Upgrade the model with [`fetched`]'s download.
    async fn upgrade(&self) -> UpgradeOutcome {
        let dir = self.dir.path().to_path_buf();
        let download =
            move |request: CliUpdateRequest, _| std::future::ready(Ok(fetched(&dir, &request)));
        self.ops
            .apply_upgrade_with(self.id, None, newer, download)
            .await
            .expect("an upgrade")
    }

    async fn model(&self) -> gglib_core::Model {
        crate::helpers::resolve_model(self.core.models(), self.id)
            .await
            .unwrap()
    }
}

#[tokio::test]
async fn an_unlinked_model_is_linked_to_the_projector_its_upgrade_brought() {
    let library = library(None).await;

    assert!(library.upgrade().await.updated);

    let model = library.model().await;
    let projector = library.dir.path().join(PROJECTOR).canonicalize().unwrap();
    assert_eq!(model.projector_path, Some(projector));
    assert_eq!(model.hf_commit_sha.as_deref(), Some(NEWER));
}

#[tokio::test]
async fn a_linked_model_keeps_its_link() {
    let library = library(Some("chosen-mmproj-F16.gguf")).await;
    let chosen = library.model().await.projector_path;

    library.upgrade().await;

    assert!(chosen.is_some());
    assert_eq!(library.model().await.projector_path, chosen);
}

#[tokio::test]
async fn the_rows_hold_the_new_oids() {
    let library = library(None).await;

    library.upgrade().await;

    let rows = library
        .core
        .verification()
        .files_of(library.id)
        .await
        .unwrap();
    let rows: Vec<_> = rows
        .iter()
        .map(|row| (row.file_path.as_str(), row.hf_oid.as_deref()))
        .collect();
    assert_eq!(
        rows,
        [(WEIGHTS, Some("new-oid")), (PROJECTOR, Some("p-oid"))]
    );
}

/// The owner renames the model while its files are being fetched. The
/// upgrade writes back what it rewrites and nothing else: the row is the
/// same one, the only one, and it keeps the new name and every field its
/// owner set.
#[tokio::test]
async fn the_model_keeps_its_row_and_its_owners_fields_set_during_the_download() {
    let library = library(None).await;
    let (core, id) = (Arc::clone(&library.core), library.id);
    let dir = library.dir.path().to_path_buf();
    let download = move |request: CliUpdateRequest, _| async move {
        let mut model = crate::helpers::resolve_model(core.models(), id).await?;
        model.name = "renamed while downloading".to_string();
        core.models().update(&model).await?;
        Ok(fetched(&dir, &request))
    };

    library
        .ops
        .apply_upgrade_with(id, None, newer, download)
        .await
        .expect("an upgrade");

    let models = library.core.models().list().await.unwrap();
    assert_eq!(models.len(), 1, "an upgrade adds no model");
    let model = &models[0];
    assert_eq!(model.id, id);
    assert_eq!(model.name, "renamed while downloading");
    assert_eq!(model.tags, ["mine"]);
    let defaults = model.inference_defaults.as_ref().expect("its defaults");
    assert_eq!(defaults.temperature, Some(0.3));
    assert_eq!(model.quantization.as_deref(), Some("Q8_0"));
    assert_eq!(model.hf_commit_sha.as_deref(), Some(NEWER));
}
