//! Tests for what the manager does with an upgrade: the plan each of its
//! files is fetched with, the copy on disk it leaves for the fetch to
//! replace, and what it hands the registrar once every file is in.

use std::path::Path;
use std::sync::Mutex as StdMutex;

use async_trait::async_trait;
use gglib_core::RepositoryError;
use gglib_core::domain::{Model, NewModel};
use gglib_core::ports::{CompletedDownload, ModelRegistrarPort, RegisteredDownload};

use super::test_support::{Started, start_next};
use super::*;
use crate::test_hub::RepoHub;

const REPO: &str = "owner/zeta-GGUF";
const FILES: [(&str, u64); 2] = [("mmproj-F16.gguf", 300), ("zeta.Q8_0.gguf", 1_000)];
/// The model in the library the upgrade is of.
const MODEL: i64 = 42;
/// The repository's latest commit, which the upgrade fetches.
const COMMIT: &str = "2222222222222222222222222222222222222222";

/// Keeps each download it is handed, with the method it came through and
/// the model an upgrade named.
#[derive(Default)]
struct RecordingRegistrar {
    calls: StdMutex<Vec<(&'static str, Option<i64>, CompletedDownload)>>,
}

impl RecordingRegistrar {
    fn keep(&self, method: &'static str, model: Option<i64>, download: &CompletedDownload) {
        let call = (method, model, download.clone());
        self.calls.lock().unwrap().push(call);
    }
}

/// The model a download is stored as, under `id`.
fn stored(id: i64, download: &CompletedDownload) -> RegisteredDownload {
    let new = NewModel::new(
        "zeta".to_string(),
        download.primary_path.clone(),
        7.0,
        chrono::Utc::now(),
    );
    RegisteredDownload {
        model: Model::stored(id, &new),
        metadata_refusal: None,
        projector_refusal: None,
        component_refusals: Vec::new(),
    }
}

#[async_trait]
impl ModelRegistrarPort for RecordingRegistrar {
    async fn register_model(
        &self,
        download: &CompletedDownload,
    ) -> Result<RegisteredDownload, RepositoryError> {
        self.keep("register_model", None, download);
        Ok(stored(1, download))
    }

    async fn upgrade_model(
        &self,
        model_id: i64,
        download: &CompletedDownload,
    ) -> Result<RegisteredDownload, RepositoryError> {
        self.keep("upgrade_model", Some(model_id), download);
        Ok(stored(model_id, download))
    }
}

/// Keeps the id of every model announced as updated.
#[derive(Default)]
struct Updated(StdMutex<Vec<i64>>);

impl AppEventEmitter for Updated {
    fn emit(&self, event: AppEvent) {
        if let AppEvent::ModelUpdated { model } = event {
            self.0.lock().unwrap().push(model.id);
        }
    }
}

struct Fixture {
    manager: DownloadManagerImpl,
    registrar: Arc<RecordingRegistrar>,
    updated: Arc<Updated>,
    _models: tempfile::TempDir,
}

/// A manager over a repository of [`FILES`], whose downloads go under a
/// directory of their own.
fn manager() -> Fixture {
    let models = tempfile::tempdir().expect("a models directory");
    let registrar = Arc::new(RecordingRegistrar::default());
    let updated = Arc::new(Updated::default());
    let config = DownloadManagerConfig {
        models_directory: Some(models.path().to_path_buf()),
        ..DownloadManagerConfig::default()
    };
    let manager = DownloadManagerImpl::new(
        registrar.clone(),
        Arc::new(RepoHub::new(&FILES)),
        updated.clone(),
        config,
        Arc::new(gglib_core::ports::NoopGgufParser),
    );
    Fixture {
        manager,
        registrar,
        updated,
        _models: models,
    }
}

/// [`manager`], with the upgrade of [`MODEL`] to [`COMMIT`] queued.
async fn upgrade_queued() -> Fixture {
    let f = manager();
    f.manager
        .queue_upgrade_download(REPO, "Q8_0", MODEL, COMMIT.to_string())
        .await
        .expect("the upgrade is queued");
    f
}

/// How a file's plan fetches it: at which revision, and whether over a copy
/// on disk.
#[derive(Debug, PartialEq, Eq)]
struct Fetched {
    revision: String,
    force: bool,
}

impl Fixture {
    /// The job the runner hands the worker for `started`'s file.
    fn job(&self, started: &Started) -> DownloadJob {
        let destination = self.manager.destination(&started.item).unwrap();
        let (progress_tx, _) = watch::channel(ProgressUpdate::default());
        DownloadManagerImpl::job_for(
            &started.item,
            destination,
            started.cancel.clone(),
            progress_tx,
        )
    }

    /// Start the next file, plan its fetch as the worker does, and end it
    /// on disk with what the worker answers for it. Answers how it was
    /// fetched.
    async fn finish_next(&self) -> Fetched {
        let started = start_next(&self.manager).await;
        let job = self.job(&started);
        let deps = WorkerDeps {
            config: self.manager.config.clone(),
        };
        let file = job.destination.files[0].clone();
        let plan = worker::plan_for(&job, &file, &deps, Arc::new(|_| {}), Arc::new(|_| {}));
        let fetched = Fetched {
            revision: plan.revision.to_string(),
            force: plan.force,
        };
        let completed = worker::completed(&job).expect("the job has its file");
        self.manager
            .finalize_job(&started.item, started.lease, Ok(completed))
            .await;
        fetched
    }

    fn calls(&self) -> Vec<(&'static str, Option<i64>, CompletedDownload)> {
        self.registrar.calls.lock().unwrap().clone()
    }
}

/// Every file of an upgrade, the projector's too, is fetched at its commit,
/// and fetched again over the copy on disk. A download that adds a model
/// fetches `main`, and keeps a file already there.
#[tokio::test]
async fn an_upgrades_files_are_fetched_again_at_its_commit() {
    let at_the_commit = Fetched {
        revision: COMMIT.to_string(),
        force: true,
    };
    let f = upgrade_queued().await;
    assert_eq!(f.finish_next().await, at_the_commit, "the weights");
    assert_eq!(f.finish_next().await, at_the_commit, "the projector");

    let f = manager();
    f.manager
        .queue_download_smart(REPO, Some("Q8_0".to_string()))
        .await
        .unwrap();
    let on_main = Fetched {
        revision: "main".to_string(),
        force: false,
    };
    assert_eq!(f.finish_next().await, on_main);
}

/// Once its last file is in, an upgrade rewrites its model's row, and adds
/// none: the registrar is asked to upgrade that model, once, with the
/// commit as the revision to record, bare. The model is announced as
/// updated.
#[tokio::test]
async fn an_upgrade_rewrites_its_models_row_at_the_commit() {
    let f = upgrade_queued().await;

    f.finish_next().await;
    assert!(f.calls().is_empty(), "the projector is still to come");
    f.finish_next().await;

    let calls = f.calls();
    let methods: Vec<_> = calls
        .iter()
        .map(|(method, model, _)| (*method, *model))
        .collect();
    assert_eq!(methods, [("upgrade_model", Some(MODEL))]);
    let download = &calls[0].2;
    assert_eq!(download.commit_sha, COMMIT);
    assert!(download.primary_path.ends_with("zeta.Q8_0.gguf"));
    assert!(download.projector_path.is_some());
    assert_eq!(*f.updated.0.lock().unwrap(), [MODEL]);
}

/// A download that adds a model is registered as one, under the revision a
/// download that names none records, and announces no model as updated.
#[tokio::test]
async fn a_download_that_adds_a_model_registers_it() {
    let f = manager();
    f.manager
        .queue_download_smart(REPO, Some("Q8_0".to_string()))
        .await
        .unwrap();

    f.finish_next().await;
    f.finish_next().await;

    let calls = f.calls();
    let methods: Vec<_> = calls
        .iter()
        .map(|(method, model, _)| (*method, *model))
        .collect();
    assert_eq!(methods, [("register_model", None)]);
    assert_eq!(calls[0].2.commit_sha, "rev:main");
    assert!(f.updated.0.lock().unwrap().is_empty());
}

/// The file on disk is the old revision's, of another size than the Hub
/// lists now. A download that adds a model deletes such a file before it
/// fetches; an upgrade leaves it for its new copy to replace.
#[tokio::test]
async fn an_upgrade_leaves_the_old_file_for_its_new_copy_to_replace() {
    let old: &[u8] = b"GGUF old revision";
    let f = upgrade_queued().await;
    let started = start_next(&f.manager).await;
    let path = f.job(&started).destination.primary_path().unwrap();
    write(&path, old);

    DownloadManagerImpl::remove_corrupt_cached_file(&started.item, Some(&path));
    assert_eq!(std::fs::read(&path).unwrap(), old, "the upgrade keeps it");

    let added = started.item.upgrading(None);
    DownloadManagerImpl::remove_corrupt_cached_file(&added, Some(&path));
    assert!(!path.exists(), "a download that adds a model deletes it");
}

/// An upgrade of files whose download is already waiting would attach to
/// that download, which adds a model: it is refused, and nothing more is
/// queued.
#[tokio::test]
async fn an_upgrade_of_files_already_queued_is_refused() {
    let f = manager();
    f.manager
        .queue_download_smart(REPO, Some("Q8_0".to_string()))
        .await
        .unwrap();

    let refused = f
        .manager
        .queue_upgrade_download(REPO, "Q8_0", MODEL, COMMIT.to_string())
        .await;

    assert!(
        matches!(refused, Err(DownloadError::AlreadyQueued { .. })),
        "{refused:?}"
    );
    let item = f.manager.queue.write().await.dequeue().unwrap();
    assert_eq!(item.upgrade, None);
}

fn write(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}
