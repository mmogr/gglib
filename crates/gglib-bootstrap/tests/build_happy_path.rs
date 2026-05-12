mod common;

use chrono::Utc;
use gglib_core::NewModel;
use std::path::PathBuf;
use tempfile::TempDir;

use common::{build_core, minimal_config, noop_emitter};

/// Bootstrapping with valid config must succeed.
#[tokio::test]
async fn build_succeeds_and_db_is_live() {
    let dir = TempDir::new().unwrap();
    let core = build_core(&dir).await;
    assert!(core.repos.models.list().await.is_ok());
}

/// Two independent builds targeting separate directories must not share data.
#[tokio::test]
async fn repos_are_isolated_between_separate_builds() {
    let dir1 = TempDir::new().unwrap();
    let dir2 = TempDir::new().unwrap();

    let build1 = build_core(&dir1).await;
    let model = NewModel::new(
        "IsolationTest".to_string(),
        dir1.path().join("model.gguf"),
        7.0,
        Utc::now(),
    );
    build1.repos.models.insert(&model).await.unwrap();
    assert_eq!(build1.repos.models.list().await.unwrap().len(), 1);

    let build2 = build_core(&dir2).await;
    assert!(build2.repos.models.list().await.unwrap().is_empty());
}

/// Providing an HF token must not cause a build failure.
#[tokio::test]
async fn hf_token_config_accepted() {
    let dir = TempDir::new().unwrap();
    let mut cfg = minimal_config(&dir);
    cfg.hf_token = Some("test_token_abc".to_string());
    assert!(
        gglib_bootstrap::CoreBootstrap::build(cfg, noop_emitter())
            .await
            .is_ok()
    );
}

/// Setting max_concurrent to a value greater than 1 must not cause a build failure.
#[tokio::test]
async fn max_concurrent_config_accepted() {
    let dir = TempDir::new().unwrap();
    let mut cfg = minimal_config(&dir);
    cfg.max_concurrent = 4;
    assert!(
        gglib_bootstrap::CoreBootstrap::build(cfg, noop_emitter())
            .await
            .is_ok()
    );
}

/// A non-existent llama-server binary is accepted at build time; failure
/// is deferred to the point where the runner is actually invoked.
#[tokio::test]
async fn nonexistent_llama_server_path_is_accepted() {
    let dir = TempDir::new().unwrap();
    let mut cfg = minimal_config(&dir);
    cfg.llama_server_path = PathBuf::from("/does/not/exist/llama-server");
    assert!(
        gglib_bootstrap::CoreBootstrap::build(cfg, noop_emitter())
            .await
            .is_ok()
    );
}

/// build() result includes a populated BuiltCore — spot-check the runner Arc.
#[tokio::test]
async fn built_core_runner_is_present() {
    let dir = TempDir::new().unwrap();
    let core = build_core(&dir).await;
    // If runner is behind an Arc<dyn …>, cloning the Arc is a proxy for "it's there".
    let _runner = core.runner.clone();
}
