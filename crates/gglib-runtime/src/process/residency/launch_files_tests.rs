//! A launch whose weights or projector are not on disk is refused by the
//! missing file's name, before anything is spawned. An image model's own
//! checks are in `launch_sd_tests.rs`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gglib_core::cache_config::CacheRamSetting;
use gglib_core::ports::{LaunchOverrides, ModelLaunchSpec, ModelRuntimeError};
use gglib_core::server_config::ServerConfigOptions;
use tokio::sync::RwLock;

use super::ensure_present;
use crate::process::RuntimeBinaries;
use crate::process::core::GuiProcessCore;
use crate::process::residency::ResidentSet;
use crate::process::residency::residency_tests::{OneModel, launch_spec};

fn spec(weights: &Path, projector: Option<&Path>) -> ModelLaunchSpec {
    ModelLaunchSpec {
        file_path: weights.to_path_buf(),
        projector: projector.map(Path::to_path_buf),
        ..launch_spec(3, "qwen")
    }
}

fn file(dir: &tempfile::TempDir, name: &str) -> PathBuf {
    let path = dir.path().join(name);
    std::fs::write(&path, b"x").unwrap();
    path
}

fn missing_file(error: ModelRuntimeError) -> String {
    match error {
        ModelRuntimeError::ModelFileNotFound(path) => path,
        other => panic!("expected ModelFileNotFound, got {other:?}"),
    }
}

#[tokio::test]
async fn weights_and_projector_on_disk_pass() {
    let dir = tempfile::tempdir().unwrap();
    let weights = file(&dir, "qwen.Q8_0.gguf");
    let projector = file(&dir, "mmproj-F16.gguf");

    assert!(ensure_present(&spec(&weights, None)).await.is_ok());
    assert!(
        ensure_present(&spec(&weights, Some(&projector)))
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn a_missing_projector_is_named() {
    let dir = tempfile::tempdir().unwrap();
    let weights = file(&dir, "qwen.Q8_0.gguf");
    let projector = dir.path().join("mmproj-F16.gguf");

    let refused = ensure_present(&spec(&weights, Some(&projector)))
        .await
        .unwrap_err();

    assert_eq!(missing_file(refused), projector.display().to_string());
}

/// The weights are checked first, so a model missing both is reported by its
/// weights, as it was before it had a projector.
#[tokio::test]
async fn missing_weights_are_named_before_a_missing_projector() {
    let dir = tempfile::tempdir().unwrap();
    let weights = dir.path().join("qwen.Q8_0.gguf");
    let projector = dir.path().join("mmproj-F16.gguf");

    let refused = ensure_present(&spec(&weights, Some(&projector)))
        .await
        .unwrap_err();

    assert_eq!(missing_file(refused), weights.display().to_string());
}

/// Through the whole admission: the weights are there and the projector is
/// not. The refusal names the projector, and it comes from the file check,
/// not from a spawn: the server binary here does not exist, so a launch that
/// got as far as spawning would fail with a different error.
#[tokio::test]
async fn an_admission_with_a_missing_projector_is_refused_before_any_spawn() {
    let dir = tempfile::tempdir().unwrap();
    let weights = file(&dir, "qwen.Q8_0.gguf");
    let projector = dir.path().join("qwen.mmproj-Q8_0.gguf");
    let set = ResidentSet::new(
        Arc::new(OneModel(spec(&weights, Some(&projector)))),
        ServerConfigOptions::default(),
        CacheRamSetting::Auto,
    );
    let core = Arc::new(RwLock::new(GuiProcessCore::new(
        19_400,
        RuntimeBinaries::llama_only("/nonexistent/llama-server"),
    )));

    let refused = set
        .admit(&core, "qwen", None, Some(4096), LaunchOverrides::default())
        .await
        .map(|_| ())
        .unwrap_err();

    assert_eq!(missing_file(refused), projector.display().to_string());
    assert_eq!(core.read().await.count(), 0, "nothing was spawned");
    assert!(set.current_model().is_none());
}

/// A stand-in for llama-server that writes its arguments, one to a line, to
/// `args.txt` beside itself and exits.
#[cfg(unix)]
fn argument_recorder(dir: &tempfile::TempDir) -> (PathBuf, PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let recorded = dir.path().join("args.txt");
    let binary = dir.path().join("llama-server");
    let script = format!(
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}.tmp'\nmv '{}.tmp' '{}'\n",
        recorded.display(),
        recorded.display(),
        recorded.display()
    );
    std::fs::write(&binary, script).unwrap();
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
    (binary, recorded)
}

/// The launch itself, as far as the spawn: the server is started with
/// `--mmproj` and the linked file. The stand-in exits at once, so the launch
/// then fails, which is not what this reads.
#[cfg(unix)]
#[tokio::test]
async fn a_launch_starts_the_server_with_the_models_projector() {
    gglib_core::paths::isolate_data_root();
    let dir = tempfile::tempdir().unwrap();
    let weights = file(&dir, "qwen.Q8_0.gguf");
    let projector = file(&dir, "qwen.mmproj-Q8_0.gguf");
    let (binary, recorded) = argument_recorder(&dir);
    let linked = ModelLaunchSpec {
        id: 999_101,
        ..spec(&weights, Some(&projector))
    };
    let set = ResidentSet::new(
        Arc::new(OneModel(linked)),
        ServerConfigOptions::default(),
        CacheRamSetting::Auto,
    );
    let core = Arc::new(RwLock::new(GuiProcessCore::new(
        19_410,
        RuntimeBinaries::llama_only(binary.to_string_lossy()),
    )));

    let failed = set
        .admit(&core, "qwen", None, Some(4096), LaunchOverrides::default())
        .await
        .map(|_| ());
    assert!(failed.is_err(), "the stand-in never becomes healthy");

    let args = std::fs::read_to_string(&recorded).expect("the stand-in ran");
    let args: Vec<&str> = args.lines().collect();
    let flag = args
        .iter()
        .position(|arg| *arg == "--mmproj")
        .unwrap_or_else(|| panic!("--mmproj missing from {args:?}"));
    assert_eq!(args[flag + 1], projector.to_string_lossy());
    let model = args.iter().position(|arg| *arg == "-m").expect("-m");
    assert_eq!(args[model + 1], weights.to_string_lossy());
}
