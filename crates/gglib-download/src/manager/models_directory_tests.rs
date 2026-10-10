//! The models directory a download goes under: the one current as the
//! download starts, for every file of it.
//!
//! Current is what `resolve_models_dir` answers in this process:
//! `GGLIB_MODELS_DIR` in its environment, then the directory stored under
//! its data root. A test binary has one data root for every test in it, and
//! a variable in the environment of whoever runs the tests would outrank
//! anything stored there. So each test here runs itself again in a process
//! of its own, started with the variable it needs or with none, and does its
//! work there, under a root nothing is stored in yet.

use std::path::{Path, PathBuf};
use std::process::Command;

use gglib_core::paths::{DirectoryCreationStrategy, isolate_data_root, set_models_dir};
use gglib_core::ports::NoopEmitter;

use super::super::group_registration_tests::RecordingRegistrar;
use super::super::runner_tests::ended;
use super::super::test_support::{End, Started, end_started, start_next};
use super::super::*;
use crate::test_hub::RepoHub;

const REPO: &str = "owner/zeta-GGUF";
const WEIGHTS: &str = "zeta.Q8_0.gguf";
const PROJECTOR: &str = "mmproj-F16.gguf";

/// What a file already on disk holds. It starts as a GGUF file does and is
/// the size the Hub lists, so the worker sent to it takes it for fetched and
/// asks the network for nothing.
const ON_DISK: &[u8; 8] = b"GGUFzeta";

/// Set in the environment of the process a test here starts, to tell it that
/// it is that process.
const STARTED_FOR_ONE_TEST: &str = "GGLIB_TEST_STARTED_FOR_A_MODELS_DIRECTORY";

/// Whether this is the process started for `test`. When it is not, that
/// process is started, with `in_environment` as its `GGLIB_MODELS_DIR` or
/// with none, and has to run the test and pass it.
fn started_for(test: &str, in_environment: Option<&Path>) -> bool {
    if std::env::var_os(STARTED_FOR_ONE_TEST).is_some() {
        return true;
    }
    let (_, module) = module_path!().split_once("::").expect("a module path");
    let home = tempfile::tempdir().expect("a home");
    let mut again = Command::new(std::env::current_exe().expect("this test binary"));
    again
        .args(["--exact", &format!("{module}::{test}")])
        .env(STARTED_FOR_ONE_TEST, "1")
        // Each test stores a directory before it downloads, so none resolves
        // the default one. A fault that did would find it here, not in the
        // home of whoever runs the tests.
        .env("HOME", home.path());
    match in_environment {
        Some(named) => again.env("GGLIB_MODELS_DIR", named),
        None => again.env_remove("GGLIB_MODELS_DIR"),
    };
    let ran = again.output().expect("the test binary runs");
    let said = String::from_utf8_lossy(&ran.stdout);
    let erred = String::from_utf8_lossy(&ran.stderr);
    assert!(ran.status.success(), "{said}\n{erred}");
    assert!(said.contains("1 passed"), "the test did not run: {said}");
    false
}

/// This process's data root, with no models directory stored under it.
fn fresh_root() -> &'static Path {
    let root = isolate_data_root();
    assert!(!root.join(".env").exists(), "the root is not fresh");
    root
}

/// Store `models` as the models directory: what the settings page's save
/// and `gglib config models-dir set` both run.
fn store(models: &Path) {
    set_models_dir(
        &models.to_string_lossy(),
        DirectoryCreationStrategy::AutoCreate,
    )
    .expect("stored");
}

/// The folder a download of [`REPO`] goes in under `models`.
fn model_folder(models: &Path) -> PathBuf {
    models.join("owner_zeta-GGUF")
}

/// Put `files` of [`REPO`] under `models`, as a download there leaves them.
fn put_on_disk(models: &Path, files: &[&str]) {
    let folder = model_folder(models);
    std::fs::create_dir_all(&folder).expect("the model's folder");
    for file in files {
        std::fs::write(folder.join(file), ON_DISK).expect("the file");
    }
}

/// A manager over a repository of `files`, handed no models directory of
/// its own, as `CoreBootstrap` hands it none.
fn manager(files: &[(&str, u64)]) -> (Arc<DownloadManagerImpl>, Arc<RecordingRegistrar>) {
    let registrar = Arc::new(RecordingRegistrar::default());
    let manager = DownloadManagerImpl::new(
        registrar.clone(),
        Arc::new(RepoHub::new(files)),
        Arc::new(NoopEmitter::new()),
        DownloadManagerConfig::default(),
        Arc::new(gglib_core::ports::NoopGgufParser),
    );
    (Arc::new(manager), registrar)
}

/// Queue the `Q8_0` download of [`REPO`], and leave the runner unstarted.
async fn queue(manager: &DownloadManagerImpl) {
    manager
        .queue_download_smart(REPO, Some("Q8_0".to_string()))
        .await
        .expect("queued");
}

/// Fetch `started`'s file as the run loop does, and answer the folder the
/// worker was sent to. The test has put the file there already, so the
/// worker asks the network for nothing.
async fn fetched_into(manager: &DownloadManagerImpl, started: &Started) -> PathBuf {
    let (progress, _) = watch::channel(ProgressUpdate::default());
    let fetched = manager
        .fetch(&started.item, started.cancel.clone(), progress)
        .await
        .expect("fetched");
    let folder = fetched.primary_path.parent().expect("a folder");
    folder.to_path_buf()
}

/// The folder `started`'s file is planned for, with nothing fetched and so
/// nothing made there.
fn planned_for(manager: &DownloadManagerImpl, started: &Started) -> PathBuf {
    let destination = manager.destination(&started.item);
    destination.expect("somewhere to go").model_dir
}

/// Download [`REPO`]'s weights through the port and the runner, and answer
/// where the registrar was told they are.
async fn download(manager: &Arc<DownloadManagerImpl>, registrar: &RecordingRegistrar) -> PathBuf {
    let id = Arc::clone(manager)
        .queue_smart(REPO.to_string(), Some("Q8_0".to_string()))
        .await
        .expect("queued");
    let outcome = ended(manager, &id).await.outcome;
    assert!(
        matches!(outcome, DownloadOutcome::Completed { .. }),
        "{outcome:?}"
    );
    let registered = registrar.registered.lock().unwrap();
    registered.last().expect("registered").primary_path.clone()
}

/// What was met in the app: a models directory saved in Settings, and the
/// download started straight afterwards landed in the old one, because the
/// daemon downloaded where the directory had resolved as it started.
///
/// The manager is built with one directory stored. The next is stored as the
/// page's save stores it, and the one after as a terminal's
/// `gglib config models-dir set` leaves it: in another process, so that the
/// file changes and nothing in this one is told.
#[tokio::test]
async fn a_models_directory_stored_while_the_manager_runs_is_where_its_next_download_goes() {
    let this = "a_models_directory_stored_while_the_manager_runs_is_where_its_next_download_goes";
    if !started_for(this, None) {
        return;
    }
    let root = fresh_root();
    let [old, saved, set] = ["old", "saved in settings", "set in a terminal"]
        .map(|name| root.join(format!("{name} models")));
    store(&set);
    let stored_by_the_terminal = std::fs::read(root.join(".env")).expect("the stored file");
    store(&old);
    let (manager, registrar) = manager(&[(WEIGHTS, 8)]);
    for models in [&old, &saved, &set] {
        put_on_disk(models, &[WEIGHTS]);
    }

    store(&saved);
    let after_the_save = download(&manager, &registrar).await;
    std::fs::write(root.join(".env"), stored_by_the_terminal).expect("the stored file");
    let after_the_command = download(&manager, &registrar).await;

    assert_eq!(after_the_save, model_folder(&saved).join(WEIGHTS));
    assert_eq!(after_the_command, model_folder(&set).join(WEIGHTS));
}

/// A download is every file of one model, and they are one folder's. Its
/// weights go under the directory current as it starts, and a directory
/// stored before they are finalized does not move the projector that
/// follows; the same download queued once it has ended is a download
/// starting then.
///
/// Each file is fetched as the run loop fetches it. Both files are on disk
/// under both directories, so a file sent to the wrong one is found there,
/// and what fails is the folder it was sent to.
#[tokio::test]
async fn a_download_part_fetched_keeps_the_models_directory_it_started_under() {
    let this = "a_download_part_fetched_keeps_the_models_directory_it_started_under";
    if !started_for(this, None) {
        return;
    }
    let root = fresh_root();
    let (old, new) = (root.join("old models"), root.join("new models"));
    store(&old);
    let (manager, _) = manager(&[(PROJECTOR, 8), (WEIGHTS, 8)]);
    for models in [&old, &new] {
        put_on_disk(models, &[WEIGHTS, PROJECTOR]);
    }
    queue(&manager).await;

    let weights = start_next(&manager).await;
    let weights_folder = fetched_into(&manager, &weights).await;
    store(&new);
    end_started(&manager, weights, End::OnDisk).await;
    let projector = start_next(&manager).await;
    let projector_folder = fetched_into(&manager, &projector).await;
    end_started(&manager, projector, End::OnDisk).await;
    queue(&manager).await;
    let again = start_next(&manager).await;
    let again_folder = fetched_into(&manager, &again).await;

    assert_eq!(weights_folder, model_folder(&old));
    assert_eq!(projector_folder, model_folder(&old));
    assert_eq!(again_folder, model_folder(&new));
}

/// The environment a process was started with outranks the stored
/// directory, for a download as for every other reader of it: a directory
/// stored while the manager runs does not move the downloads of a daemon
/// whose own environment names one.
#[tokio::test]
async fn a_models_directory_in_the_environment_outranks_one_stored_while_the_manager_runs() {
    let this = "a_models_directory_in_the_environment_outranks_one_stored_while_the_manager_runs";
    let named = std::env::temp_dir().join("gglib models named in the environment");
    if !started_for(this, Some(&named)) {
        return;
    }
    let root = fresh_root();
    store(&root.join("old models"));
    let (manager, _) = manager(&[(WEIGHTS, 1_000)]);

    store(&root.join("new models"));
    queue(&manager).await;
    let weights = start_next(&manager).await;

    assert_eq!(planned_for(&manager, &weights), model_folder(&named));
}
