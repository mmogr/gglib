//! Tests that verify and repair read a model's files where its download
//! wrote them, when its weights sit in a repository subdirectory, and that
//! each file is named by what it is.

use std::path::Path;
use std::sync::{Arc, Mutex};

use chrono::Utc;

use super::tests::{REPO, Rows, row, sha256};
use crate::domain::{Model, ModelFile, NewModel};
use crate::ports::AskedDownloads;
use crate::ports::huggingface::fake_hub::{FakeHub, hub_file};
use crate::services::model_projector::tests::OneModelRepo;
use crate::services::{
    ModelVerificationService, OverallHealth, ShardHealth, ShardProgress, VerifiedFileKind,
};

const FIRST: &str = "BF16/zeta-BF16-00001-of-00002.gguf";
const SECOND: &str = "BF16/zeta-BF16-00002-of-00002.gguf";
const PROJECTOR: &str = "mmproj-BF16.gguf";

/// A `BF16` download of [`REPO`] into `dir`, as a download writes it: the
/// two shards under `BF16/`, the projector at the root. Each file holds its
/// own name, and its row holds that content's hash.
fn downloaded_into(dir: &Path) -> Vec<ModelFile> {
    std::fs::create_dir_all(dir.join("BF16")).unwrap();
    [FIRST, SECOND, PROJECTOR]
        .into_iter()
        .zip(0..)
        .map(|(name, index)| {
            std::fs::write(dir.join(name), name).unwrap();
            row(index, name, &sha256(name))
        })
        .collect()
}

/// The service for model 1, whose file is the first shard in `dir`, over
/// `rows`.
fn service(
    dir: &Path,
    rows: Vec<ModelFile>,
    queued: Arc<AskedDownloads>,
) -> ModelVerificationService {
    let mut new = NewModel::new("zeta".to_owned(), dir.join(FIRST), 27.0, Utc::now());
    new.hf_repo_id = Some(REPO.to_owned());
    new.quantization = Some("BF16".to_owned());
    let hub = FakeHub {
        weights: vec![hub_file(FIRST, 1, "a"), hub_file(SECOND, 1, "b")],
        projectors: vec![hub_file(PROJECTOR, 1, "p")],
        ..Default::default()
    };
    ModelVerificationService::new(
        Arc::new(OneModelRepo(Mutex::new(Model::stored(1, &new)))),
        Arc::new(Rows(rows)),
        Arc::new(hub),
        queued,
    )
}

#[tokio::test]
async fn shards_in_a_subdirectory_and_a_projector_at_the_root_are_healthy() {
    let dir = tempfile::tempdir().unwrap();
    let rows = downloaded_into(dir.path());
    let service = service(dir.path(), rows, Arc::default());

    let (mut progress, handle) = service.verify_model_integrity(1).await.unwrap();
    let mut started = Vec::new();
    while let Some(update) = progress.recv().await {
        if matches!(update.shard_progress, ShardProgress::Starting) {
            started.push(update.kind.label());
        }
    }
    let report = handle.await.unwrap().unwrap();

    assert_eq!(report.overall_health, OverallHealth::Healthy);
    let files: Vec<_> = report
        .shards
        .iter()
        .map(|file| (file.index, file.kind, file.file_path.as_str(), &file.health))
        .collect();
    assert_eq!(
        files,
        [
            (
                0,
                VerifiedFileKind::Shard { number: 1, of: 2 },
                FIRST,
                &ShardHealth::Healthy
            ),
            (
                1,
                VerifiedFileKind::Shard { number: 2, of: 2 },
                SECOND,
                &ShardHealth::Healthy
            ),
            (
                2,
                VerifiedFileKind::Projector,
                PROJECTOR,
                &ShardHealth::Healthy
            ),
        ]
    );
    assert_eq!(started, ["Shard 1/2", "Shard 2/2", "Projector"]);
}

/// A repair finds the corrupt shard where the download wrote it, deletes
/// that file and leaves the healthy ones.
#[tokio::test]
async fn a_repair_deletes_the_corrupt_shard_from_the_subdirectory() {
    let dir = tempfile::tempdir().unwrap();
    let rows = downloaded_into(dir.path());
    std::fs::write(dir.path().join(SECOND), "damaged").unwrap();
    let queued = Arc::new(AskedDownloads::default());
    let service = service(dir.path(), rows, Arc::clone(&queued));

    let started = service.repair_model(1, None).await.unwrap();

    assert_eq!(started.files, [SECOND]);
    assert!(!dir.path().join(SECOND).exists());
    assert!(dir.path().join(FIRST).exists());
    assert!(dir.path().join(PROJECTOR).exists());
    assert_eq!(queued.asked(), [(REPO.to_owned(), Some("BF16".to_owned()))]);
}

#[test]
fn a_projector_and_a_companion_are_no_shards_and_shards_count_from_one() {
    let rows = [
        row(0, "zeta.Q8_0.gguf", "w"),
        row(1, "mmproj-F16.gguf", "p"),
        row(2, "/models/other/vae.gguf", "c"),
    ];

    let kinds = VerifiedFileKind::of_rows(&rows);

    assert_eq!(
        kinds,
        [
            VerifiedFileKind::Shard { number: 1, of: 1 },
            VerifiedFileKind::Projector,
            VerifiedFileKind::Companion,
        ]
    );
    let labels: Vec<String> = kinds.into_iter().map(VerifiedFileKind::label).collect();
    assert_eq!(labels, ["Shard 1/1", "Projector", "Companion"]);
}
