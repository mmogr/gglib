//! `apply_upgrade_with`, with a hand-written check and download in place of
//! the Hub.

use std::path::PathBuf;
use std::sync::Mutex;

use gglib_core::domain::NewModel;
use gglib_core::download::{DownloadId, RowFacts, row};
use gglib_core::ports::{NoopEmitter, NoopGgufParser, NoopModelRuntime};
use gglib_core::services::AppCore;

use super::*;
use crate::models::ModelDeps;
use crate::test_support::test_core;

const REPO: &str = "owner/zeta-GGUF";
const RECORDED: &str = "1111111111111111111111111111111111111111";
const NEWER: &str = "2222222222222222222222222222222222222222";

/// A library holding one model downloaded from [`REPO`] at [`RECORDED`],
/// and the model's id.
async fn library() -> (Arc<AppCore>, ModelOps, i64) {
    let core = test_core().await;
    let mut model = NewModel::new(
        "zeta".to_string(),
        PathBuf::from("/models/old/zeta.Q8_0.gguf"),
        8.0,
        chrono::Utc::now(),
    );
    model.hf_repo_id = Some(REPO.to_string());
    model.hf_commit_sha = Some(RECORDED.to_string());
    model.quantization = Some("Q8_0".to_string());
    let id = core.models().add(model).await.expect("a model").id;

    let ops = ModelOps::new(ModelDeps {
        core: Arc::clone(&core),
        runtime: Arc::new(NoopModelRuntime),
        gguf_parser: Arc::new(NoopGgufParser),
        emitter: Arc::new(NoopEmitter::new()),
    });
    (core, ops, id)
}

/// A sink that keeps the id of each row it is handed.
fn recording() -> (RowCallback, Arc<Mutex<Vec<String>>>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    let rows: RowCallback = Arc::new(move |row| sink.lock().unwrap().push(row.id.clone()));
    (rows, seen)
}

/// A check that finds `latest` on the Hub.
fn finds(
    repo: &str,
    recorded: Option<String>,
    latest: &str,
) -> std::future::Ready<anyhow::Result<UpdateCheckResult>> {
    assert_eq!(repo, REPO);
    std::future::ready(Ok(UpdateCheckResult {
        has_update: recorded.as_deref() != Some(latest),
        current_sha: recorded,
        latest_sha: latest.to_string(),
    }))
}

/// The download is given the caller's sink, so a row it hands over reaches
/// the caller, and what it fetched is what the model's row is rewritten to.
#[tokio::test]
async fn the_download_hands_its_row_to_the_callers_sink() {
    let (core, ops, id) = library().await;
    let (rows, seen) = recording();
    let fetched = PathBuf::from("/models/new/zeta.Q8_0.gguf");

    let landed = fetched.clone();
    let download = |request: CliUpdateRequest, rows: Option<RowCallback>| async move {
        let rows = rows.expect("the caller's sink for the row");
        let download = DownloadId::new(request.repo_id.as_str(), Some(&request.quantization));
        rows(&row(&RowFacts::waiting(&download, 1, None, None)));
        Ok(CliDownloadResult {
            downloaded_paths: vec![landed.clone()],
            primary_path: landed,
            quantization: request.quantization,
            repo_id: request.repo_id,
            commit_sha: NEWER.to_string(),
        })
    };
    let outcome = ops
        .apply_upgrade_with(
            id,
            Some(rows),
            |repo, recorded, _| finds(&repo, recorded, NEWER),
            download,
        )
        .await
        .expect("an upgrade");

    assert!(outcome.updated);
    assert_eq!(outcome.latest_sha, NEWER);
    assert_eq!(*seen.lock().unwrap(), ["owner/zeta-GGUF:Q8_0"]);
    let model = crate::helpers::resolve_model(core.models(), id)
        .await
        .unwrap();
    assert_eq!(model.file_path, fetched);
    assert_eq!(model.hf_commit_sha.as_deref(), Some(NEWER));
}

/// A model already at the Hub's revision is not downloaded.
#[tokio::test]
async fn a_current_model_is_not_downloaded() {
    let (core, ops, id) = library().await;
    let (rows, seen) = recording();

    let download = |_, _| -> std::future::Ready<anyhow::Result<CliDownloadResult>> {
        panic!("a current model was downloaded")
    };
    let outcome = ops
        .apply_upgrade_with(
            id,
            Some(rows),
            |repo, recorded, _| finds(&repo, recorded, RECORDED),
            download,
        )
        .await
        .expect("a check");

    assert!(!outcome.updated);
    assert!(seen.lock().unwrap().is_empty());
    let model = crate::helpers::resolve_model(core.models(), id)
        .await
        .unwrap();
    assert_eq!(model.hf_commit_sha.as_deref(), Some(RECORDED));
}
