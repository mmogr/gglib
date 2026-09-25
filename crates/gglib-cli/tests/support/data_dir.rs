//! A data directory for a test to point `GGLIB_DATA_DIR` at, with its
//! settings written and read through the store the binary's bootstrap wires.
//!
//! Lives in a subdirectory because anything directly under `tests/` is built
//! as its own test binary; `#[path]`-included from the suites that need it.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gglib_bootstrap::{BootstrapConfig, BuiltCore, CoreBootstrap};
use gglib_core::{NoopEmitter, Settings};

/// The database the binary opens when `GGLIB_DATA_DIR` is `root`.
pub(crate) fn database(root: &Path) -> PathBuf {
    root.join("data").join("gglib.db")
}

/// Apply `change` to the settings in `root`'s database, creating the
/// database first if there is none.
pub(crate) fn write_settings(root: &Path, change: impl Fn(&mut Settings) + Send + Sync) {
    runtime().block_on(async {
        let built = open(root).await;
        built
            .repos
            .settings
            .modify(&|settings: &mut Settings| {
                change(settings);
                Ok(())
            })
            .await
            .expect("the settings are written");
        built.pool.close().await;
    });
}

/// The settings stored in `root`'s database.
pub(crate) fn read_settings(root: &Path) -> Settings {
    runtime().block_on(async {
        let built = open(root).await;
        let settings = built
            .repos
            .settings
            .load()
            .await
            .expect("the settings load");
        built.pool.close().await;
        settings
    })
}

/// `root`'s database, opened through `CoreBootstrap::build` as the binary's
/// own bootstrap opens it.
async fn open(root: &Path) -> BuiltCore {
    let models_dir = root.join("models");
    std::fs::create_dir_all(&models_dir).expect("models dir");
    let config = BootstrapConfig {
        db_path: database(root),
        llama_server_path: "/nonexistent/llama-server".into(),
        models_dir,
        hf_token: None,
    };
    CoreBootstrap::build(config, Arc::new(NoopEmitter::new()))
        .await
        .expect("the database opens")
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime")
}
