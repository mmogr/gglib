mod common;

use tempfile::TempDir;

use common::{minimal_config, noop_emitter};

/// A `db_path` whose directory cannot be made cannot be opened: a file is
/// where the directory would go, which no OS lets a directory replace.
#[tokio::test]
async fn build_fails_when_the_db_directory_cannot_be_made() {
    let dir = TempDir::new().unwrap();
    let mut cfg = minimal_config(&dir);
    let in_the_way = dir.path().join("a-file");
    std::fs::write(&in_the_way, b"").unwrap();
    cfg.db_path = in_the_way.join("gglib.db");
    let result = gglib_bootstrap::CoreBootstrap::build(cfg, noop_emitter()).await;
    assert!(
        result.is_err(),
        "expected build to fail when the db directory cannot be made"
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
