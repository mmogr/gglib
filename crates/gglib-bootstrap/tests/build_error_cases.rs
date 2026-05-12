mod common;

use std::path::PathBuf;
use tempfile::TempDir;

use common::{minimal_config, noop_emitter};

/// A `db_path` that points into a directory that does not exist cannot be opened.
#[tokio::test]
async fn build_fails_when_db_directory_does_not_exist() {
    let dir = TempDir::new().unwrap();
    let mut cfg = minimal_config(&dir);
    cfg.db_path = PathBuf::from("/nonexistent_xyz_abc_bootstrap/gglib.db");
    let result = gglib_bootstrap::CoreBootstrap::build(cfg, noop_emitter()).await;
    assert!(
        result.is_err(),
        "expected build to fail when db directory does not exist"
    );
}

/// A `db_path` that is itself a directory (not a file) cannot be used as a database.
#[tokio::test]
async fn build_fails_when_db_path_is_a_directory() {
    let dir = TempDir::new().unwrap();
    let mut cfg = minimal_config(&dir);
    // Point db_path at the temp dir itself (a directory, not a file).
    cfg.db_path = dir.path().to_path_buf();
    let result = gglib_bootstrap::CoreBootstrap::build(cfg, noop_emitter()).await;
    assert!(
        result.is_err(),
        "expected build to fail when db_path is a directory"
    );
}
