//! Through the whole admission: a resident is compared with the projector the
//! request's model is linked to now, and a launch records the one it used.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gglib_core::cache_config::CacheRamSetting;
use gglib_core::ports::{LaunchOverrides, ModelLaunchSpec, ModelRuntimeError};
use gglib_core::server_config::ServerConfigOptions;
#[cfg(unix)]
use tokio::sync::RwLock;

#[cfg(unix)]
use super::tests::answer_health;
use super::tests::healthy_port;
#[cfg(unix)]
use crate::process::RuntimeBinaries;
use crate::process::admission::{PRIMARY_SLOT, Resident};
use crate::process::core::GuiProcessCore;
use crate::process::residency::ResidentSet;
use crate::process::residency::hold_tests::{core, resident};
use crate::process::residency::residency_tests::{OneModel, launch_spec};

/// A weights file and a projector file, both on disk, so a launch gets past
/// its file check.
fn files(dir: &tempfile::TempDir) -> (PathBuf, PathBuf) {
    let weights = dir.path().join("qwen.Q8_0.gguf");
    let projector = dir.path().join("qwen.mmproj-Q8_0.gguf");
    std::fs::write(&weights, b"w").unwrap();
    std::fs::write(&projector, b"p").unwrap();
    (weights, projector)
}

/// A set whose catalog holds `qwen` (model 1) with these files, linked to
/// `projector` or to nothing.
fn set_for(weights: &Path, projector: Option<&Path>) -> ResidentSet {
    let spec = ModelLaunchSpec {
        file_path: weights.to_path_buf(),
        projector: projector.map(Path::to_path_buf),
        ..launch_spec(1, "qwen")
    };
    ResidentSet::new(
        Arc::new(OneModel(spec)),
        ServerConfigOptions::default(),
        CacheRamSetting::Auto,
    )
}

/// `qwen`, healthy, launched with `projector`.
fn healthy_resident(projector: Option<&Path>) -> Resident {
    Resident {
        projector: projector.map(Path::to_path_buf),
        runtime: gglib_core::domain::RuntimeKind::Llama,
        components: Vec::new(),
        ..resident(healthy_port())
    }
}

async fn admit(set: &ResidentSet) -> Result<bool, ModelRuntimeError> {
    set.admit(
        &core(),
        "qwen",
        None,
        Some(4096),
        LaunchOverrides::default(),
    )
    .await
    .map(|admission| admission.target.just_started)
}

/// Admits a request whose resident is recycled. The request goes on to a
/// launch, and the server binary does not exist, so it ends at the spawn with
/// the slot empty.
async fn assert_recycled(set: &ResidentSet) {
    let outcome = admit(set).await;
    assert!(
        matches!(outcome, Err(ModelRuntimeError::SpawnFailed(_))),
        "{outcome:?}"
    );
    assert!(set.queue().slot(PRIMARY_SLOT).is_none(), "recycled");
}

/// The control for the two tests after it: a resident launched with the
/// projector its model is linked to serves the request.
#[tokio::test]
async fn an_admission_is_served_by_a_resident_launched_with_the_linked_projector() {
    let dir = tempfile::tempdir().unwrap();
    let (weights, projector) = files(&dir);
    let set = set_for(&weights, Some(&projector));
    drop(
        set.queue()
            .install(PRIMARY_SLOT, healthy_resident(Some(&projector))),
    );

    let just_started = admit(&set).await.unwrap();

    assert!(!just_started, "served, not launched");
    assert!(set.queue().slot(PRIMARY_SLOT).is_some());
}

/// The model's link is what the resident is compared with: one launched
/// before the model was linked is recycled, and the request goes on to launch.
#[tokio::test]
async fn an_admission_recycles_a_resident_launched_before_its_model_was_linked() {
    let dir = tempfile::tempdir().unwrap();
    let (weights, projector) = files(&dir);
    let set = set_for(&weights, Some(&projector));
    drop(set.queue().install(PRIMARY_SLOT, healthy_resident(None)));

    assert_recycled(&set).await;
}

/// The other direction: unlinked since launch.
#[tokio::test]
async fn an_admission_recycles_a_resident_launched_before_its_model_was_unlinked() {
    let dir = tempfile::tempdir().unwrap();
    let (weights, projector) = files(&dir);
    let set = set_for(&weights, None);
    drop(
        set.queue()
            .install(PRIMARY_SLOT, healthy_resident(Some(&projector))),
    );

    assert_recycled(&set).await;
}

/// A model id no other test in this binary spawns; see `launch_tests`.
#[cfg(unix)]
const LAUNCHED_ID: u32 = 999_102;

/// A stand-in for llama-server that stays up and listens on nothing.
#[cfg(unix)]
fn idle_server(dir: &tempfile::TempDir) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let binary = dir.path().join("llama-server");
    std::fs::write(&binary, "#!/bin/sh\nexec sleep 30\n").unwrap();
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
    binary
}

/// Answers `/health` on the port `core` gives the server it spawns, once it
/// has spawned one: the stand-in cannot, and a launch waits for the answer.
#[cfg(unix)]
fn answer_health_for_the_spawned_server(core: &Arc<RwLock<GuiProcessCore>>) {
    let core = Arc::clone(core);
    tokio::spawn(async move {
        loop {
            let port = core.read().await.list_all().first().map(|info| info.port);
            if let Some(port) = port {
                answer_health(std::net::TcpListener::bind(("127.0.0.1", port)).unwrap());
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    });
}

/// A launch that completes: the resident it installs carries the projector it
/// was started with, so the next request for the same linked model is served
/// by it and nothing is launched again.
#[cfg(unix)]
#[tokio::test]
async fn a_launched_resident_records_its_projector_and_serves_the_next_request() {
    gglib_core::paths::isolate_data_root();
    let dir = tempfile::tempdir().unwrap();
    let (weights, projector) = files(&dir);
    let linked = ModelLaunchSpec {
        id: LAUNCHED_ID,
        file_path: weights,
        projector: Some(projector.clone()),
        ..launch_spec(LAUNCHED_ID, "qwen")
    };
    let set = ResidentSet::new(
        Arc::new(OneModel(linked)),
        ServerConfigOptions::default(),
        CacheRamSetting::Auto,
    );
    let core = Arc::new(RwLock::new(GuiProcessCore::new(
        19_440,
        RuntimeBinaries::llama_only(idle_server(&dir).to_string_lossy()),
    )));
    answer_health_for_the_spawned_server(&core);

    let first = set
        .admit(&core, "qwen", None, Some(4096), LaunchOverrides::default())
        .await
        .expect("the launch becomes healthy");
    assert!(first.target.just_started);
    let installed = set.queue().slot(PRIMARY_SLOT).expect("a resident");
    assert_eq!(installed.projector, Some(projector));
    // The first request ends, and its lease with it: the next is not queued
    // behind it.
    let port = first.target.port;
    drop(first);

    let second = set
        .admit(&core, "qwen", None, Some(4096), LaunchOverrides::default())
        .await
        .expect("served");
    assert!(!second.target.just_started, "the resident serves");
    assert_eq!(second.target.port, port);
    assert_eq!(core.read().await.count(), 1, "one server, launched once");

    core.write().await.kill(LAUNCHED_ID).await.ok();
}
