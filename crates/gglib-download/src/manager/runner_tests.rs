//! Tests of the port's one way of queueing: that it starts the runner, and
//! that a quantization asked for by name is the one queued.

use std::path::Path;
use std::time::Duration;

use gglib_core::download::FinishedDownload;
use gglib_core::ports::NoopEmitter;

use super::duplicate_guard_tests::NoRegistrar;
use super::*;
use crate::test_hub::RepoHub;

const REPO: &str = "owner/zeta-GGUF";

/// A manager over a repository of one `Q8_0` file, whose models directory is
/// a file. Its worker fails each download where it makes the model's folder,
/// which is before it asks the network for anything.
fn manager_that_cannot_write(dir: &Path) -> Arc<DownloadManagerImpl> {
    let blocked = dir.join("models");
    std::fs::write(&blocked, "a file where the models directory would be").unwrap();
    Arc::new(DownloadManagerImpl::new(
        Arc::new(NoRegistrar),
        Arc::new(RepoHub::new(&[("zeta.Q8_0.gguf", 1_000)])),
        Arc::new(NoopEmitter::new()),
        DownloadManagerConfig {
            models_directory: Some(blocked),
            ..DownloadManagerConfig::default()
        },
        Arc::new(gglib_core::ports::NoopGgufParser),
    ))
}

/// How the download `id` ended, once the queue says it has.
pub(super) async fn ended(manager: &DownloadManagerImpl, id: &DownloadId) -> FinishedDownload {
    let id = id.to_string();
    let read = async {
        loop {
            let snapshot = manager.get_queue_snapshot().await.unwrap();
            if let Some(ended) = snapshot.finished.into_iter().find(|ended| ended.id == id) {
                return ended;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(10), read)
        .await
        .expect("the download was never taken off the queue")
}

/// What a repair met: its download was queued where nothing had started the
/// runner, and waited there until some other download was queued. Queueing
/// through the port starts the runner itself.
#[tokio::test]
async fn a_download_queued_through_the_port_is_run_with_nothing_else_queued() {
    let dir = tempfile::tempdir().unwrap();
    let manager = manager_that_cannot_write(dir.path());

    let id = Arc::clone(&manager)
        .queue_smart(REPO.to_string(), Some("Q8_0".to_string()))
        .await
        .unwrap();

    let ended = ended(&manager, &id).await;
    let DownloadOutcome::Failed { error } = &ended.outcome else {
        panic!("the worker cannot make its folder, yet {:?}", ended.outcome);
    };
    assert!(error.starts_with("I/O error (create_dir)"), "{error}");
}

/// A repair asks for a quantization by the name the model was stored with.
/// Each name queues that quantization's own file: `UD-Q6_K` is not `Q6_K`,
/// and none of them becomes another.
#[tokio::test]
async fn a_quantization_asked_for_by_its_name_is_the_one_queued() {
    let files = [
        ("zeta.BF16.gguf", 1),
        ("zeta.F16.gguf", 2),
        ("zeta.IQ4_XS.gguf", 3),
        ("zeta.Q4_K_M.gguf", 4),
        ("zeta.Q6_K.gguf", 5),
        ("zeta.Q8_0.gguf", 6),
        ("zeta.UD-Q4_K_M.gguf", 7),
        ("zeta.UD-Q6_K.gguf", 8),
    ];
    for (file, _) in files {
        let name = file
            .strip_prefix("zeta.")
            .and_then(|rest| rest.strip_suffix(".gguf"))
            .unwrap();
        let manager = DownloadManagerImpl::new(
            Arc::new(NoRegistrar),
            Arc::new(RepoHub::new(&files)),
            Arc::new(NoopEmitter::new()),
            DownloadManagerConfig::default(),
            Arc::new(gglib_core::ports::NoopGgufParser),
        );

        let id = manager
            .queue_download_smart(REPO, Some(name.to_string()))
            .await
            .unwrap();

        assert_eq!(id.to_string(), format!("{REPO}:{name}"));
        let kept = manager.file_entries_map.lock().await[&id.to_string()].clone();
        let queued: Vec<&str> = kept.iter().map(|file| file.path.as_str()).collect();
        assert_eq!(queued, [file], "asked for {name}");
    }
}
