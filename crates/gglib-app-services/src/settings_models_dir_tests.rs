//! The models directory as [`super::SettingsOps`] reports and saves it.
//!
//! A save is stored under the data root, which is this binary's test root
//! (`isolate_data_root`): one per process, shared by every test in it. A
//! directory stored there would be what every other test here resolves from
//! then on, and a `GGLIB_MODELS_DIR` in the environment of whoever runs the
//! tests would outrank it. So the save runs in a child: the parent re-runs
//! this binary for that one test without the variable, and the child's fresh
//! root is its own. The child is chosen by name and by a variable only the
//! parent sets, so an ordinary run of it checks nothing, and the parent fails
//! on a missing marker if a rename leaves it unrun.

use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

use gglib_core::paths::default_models_dir;

use super::*;
use crate::test_support::{MockDownloadManager, MockSystemProbePort, test_core};

/// The child's path in this test binary.
const CHILD: &str =
    "settings::models_dir_tests::the_child_saves_a_models_directory_and_reads_it_back";

/// Set by the parent for the child alone.
const CHILD_ENV: &str = "GGLIB_MODELS_DIR_SAVE_CHILD";

/// Printed by the child once every check below it has passed.
const MARKER: &str = "A-SAVED-MODELS-DIRECTORY-READ-BACK";

async fn ops() -> SettingsOps {
    SettingsOps::new(SettingsDeps {
        core: test_core().await,
        system_probe: Arc::new(MockSystemProbePort::default()),
        downloads: Arc::new(MockDownloadManager::new()),
    })
}

#[test]
fn a_saved_models_directory_is_what_the_next_read_returns() {
    let out = Command::new(std::env::current_exe().expect("this test binary"))
        .args(["--exact", CHILD, "--nocapture", "--test-threads=1"])
        .env(CHILD_ENV, "1")
        .env_remove("GGLIB_MODELS_DIR")
        .output()
        .expect("re-run this binary for the child test");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(
        out.status.success(),
        "the saved directory did not read back:\n{stdout}\n{stderr}"
    );
    assert!(
        stdout.contains(MARKER),
        "the child never reported its checks:\n{stdout}\n{stderr}"
    );
}

/// Run by the parent above, alone in its process, so its test root is fresh
/// and its environment names no models directory.
#[tokio::test]
async fn the_child_saves_a_models_directory_and_reads_it_back() {
    if std::env::var(CHILD_ENV).is_err() {
        println!("not the parent's child run: nothing to check");
        return;
    }
    let root = PathBuf::from(gglib_core::paths::isolate_data_root());
    assert!(!root.join(".env").exists(), "the child's root is not fresh");
    let before = ops().await.get_models_directory_info().expect("read");
    assert_eq!(before.source, "default");
    assert_eq!(before.path, before.default_path);
    let chosen = root.join("chosen models");

    let saved = ops()
        .await
        .update_models_directory(&chosen.to_string_lossy())
        .expect("saved");

    assert!(chosen.is_dir(), "the directory is created");
    // Operations built anew over the same data root: what a restart reads.
    let read_back = ops().await.get_models_directory_info().expect("read");
    for info in [saved, read_back] {
        assert_eq!(info.path, chosen.to_string_lossy());
        assert_eq!(info.source, "environment");
        assert_eq!(info.default_path, before.default_path);
        assert!(info.exists && info.writable, "{info:?}");
    }

    println!("{MARKER}");
}

/// The default the settings page offers to reset to is the directory core
/// and the CLI fall back to, not one of the page's own.
#[tokio::test]
async fn the_default_models_directory_offered_is_cores_own() {
    let info = ops().await.get_models_directory_info().expect("read");

    assert_eq!(
        info.default_path,
        default_models_dir()
            .expect("a home directory")
            .to_string_lossy()
    );
}

/// A path that names no directory is the caller's mistake, and is refused
/// before anything is created or stored.
#[tokio::test]
async fn a_path_that_cannot_be_the_models_directory_is_refused_as_invalid() {
    let ops = ops().await;
    let file = tempfile::NamedTempFile::new().expect("a file");

    for path in [String::new(), file.path().to_string_lossy().into_owned()] {
        let refused = ops.update_models_directory(&path);

        assert!(
            matches!(refused, Err(GuiError::ValidationFailed(_))),
            "{path:?}: {refused:?}"
        );
    }
}
