//! Shared test helpers for gglib-bootstrap integration tests.
//!
//! All helpers are designed to keep individual test functions to 3–5 lines.
//! The `TempDir` returned by `minimal_config` must be kept alive for the
//! duration of the test — `SQLite` holds the file open.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use tempfile::TempDir;

use gglib_bootstrap::{BootstrapConfig, BuiltCore, CoreBootstrap};
use gglib_core::ports::{AppEventEmitter, NoopEmitter};

/// Build a minimal, valid [`BootstrapConfig`] pointing at temp-dir paths.
///
/// The caller must keep the returned [`TempDir`] alive for the duration of
/// the test; dropping it deletes the directory and will break open DB handles.
pub fn minimal_config(dir: &TempDir) -> BootstrapConfig {
    let models_dir = dir.path().join("models");
    fs::create_dir_all(&models_dir).expect("create models dir");
    BootstrapConfig {
        db_path: dir.path().join("gglib.db"),
        llama_server_path: PathBuf::from("/nonexistent/llama-server"),
        max_concurrent: 1,
        models_dir,
        hf_token: None,
    }
}

/// Return a no-op [`AppEventEmitter`] suitable for tests.
pub fn noop_emitter() -> Arc<dyn AppEventEmitter> {
    Arc::new(NoopEmitter::new())
}

/// Run [`CoreBootstrap::build`] with `minimal_config` and panic on failure.
///
/// This is the one-liner used by every happy-path and functional test.
#[allow(dead_code)]
pub async fn build_core(dir: &TempDir) -> BuiltCore {
    CoreBootstrap::build(minimal_config(dir), noop_emitter())
        .await
        .expect("CoreBootstrap::build must succeed with valid config")
}
