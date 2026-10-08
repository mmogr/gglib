//! A repair through the wired core: its download is queued on the manager
//! the adapters hold, and that manager runs it.

use std::time::Duration;

use gglib_core::domain::{NewModel, NewModelFile};
use gglib_core::download::DownloadOutcome;
use gglib_core::ports::huggingface::fake_hub::{FakeHub, hub_file};
use gglib_core::ports::{DownloadManagerConfig, NoopEmitter};

use super::*;

const WEIGHTS: &str = "zeta.Q8_0.gguf";
/// The SHA-256 of `weights`, which the file on disk does not hold.
const HEALTHY_OID: &str = "9a129038d9a00aed0cf6a7ea059ca50a813449061ab87848cf1a13eafdf33b2c";

/// `gglib model repair` deleted a model's corrupt file and queued its
/// download where nothing had started the runner, so nothing was fetched.
///
/// The models directory here is a file, so the worker fails the download as
/// it makes the model's folder, before it asks the network for anything:
/// a download that has ended is one the runner took up.
#[tokio::test]
async fn a_repair_leaves_a_running_download() {
    let dir = tempfile::tempdir().unwrap();
    let pool = setup_database(&dir.path().join("gglib.db")).await.unwrap();
    let models_dir = dir.path().join("models");
    std::fs::write(&models_dir, "a file where the models directory would be").unwrap();
    let hub = FakeHub {
        weights: vec![hub_file(WEIGHTS, 7, "w-oid")],
        ..FakeHub::default()
    };
    let built = wire(
        pool,
        Arc::new(hub),
        DownloadManagerConfig {
            models_directory: Some(models_dir),
            ..DownloadManagerConfig::default()
        },
        None,
        Arc::new(NoopEmitter::new()),
    );

    let folder = dir.path().join("library");
    std::fs::create_dir(&folder).unwrap();
    let corrupt = folder.join(WEIGHTS);
    std::fs::write(&corrupt, "damaged").unwrap();
    let mut model = NewModel::new("zeta".to_owned(), corrupt.clone(), 7.0, chrono::Utc::now());
    model.hf_repo_id = Some("owner/zeta-GGUF".to_owned());
    model.quantization = Some("Q8_0".to_owned());
    let id = built.app.models().add(model).await.unwrap().id;
    let oid = Some(HEALTHY_OID.to_owned());
    let row = NewModelFile::new(id, WEIGHTS.to_owned(), 0, 7, oid);
    built.repos.model_files.insert(&row).await.unwrap();

    let started = built
        .app
        .verification()
        .repair_model(id, None)
        .await
        .unwrap();

    assert!(!corrupt.exists(), "the corrupt file is deleted");
    assert_eq!(started.id, "owner/zeta-GGUF:Q8_0");
    assert_eq!(started.files, [WEIGHTS]);
    // Nothing more is asked of the manager, and the download ends.
    let read = async {
        loop {
            let snapshot = built.downloads.get_queue_snapshot().await.unwrap();
            let mut finished = snapshot.finished.into_iter();
            if let Some(ended) = finished.find(|ended| ended.id == started.id) {
                return ended;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    };
    let ended = tokio::time::timeout(Duration::from_secs(10), read)
        .await
        .expect("the download was left waiting on the queue");
    let DownloadOutcome::Failed { error } = &ended.outcome else {
        panic!("the worker cannot make its folder, yet {:?}", ended.outcome);
    };
    assert!(error.starts_with("I/O error (create_dir)"), "{error}");
}
