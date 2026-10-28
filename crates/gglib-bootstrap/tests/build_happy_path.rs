mod common;

use std::sync::Arc;

use chrono::Utc;
use gglib_bootstrap::CoreBootstrap;
use gglib_core::NewModel;
use gglib_core::ports::HfClientPort;
use gglib_core::ports::huggingface::fake_hub::FakeHub;
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

/// `build()` result includes a populated `BuiltCore` — spot-check the downloads Arc.
#[tokio::test]
async fn built_core_downloads_is_present() {
    let dir = TempDir::new().unwrap();
    let core = build_core(&dir).await;
    // Behind an Arc<dyn …>, cloning the Arc is a proxy for "it's there".
    let _downloads = core.downloads.clone();
}

/// A core built over a caller's Hub holds that Hub, and a live library.
#[tokio::test]
async fn a_core_built_with_a_hub_holds_that_hub() {
    let dir = TempDir::new().unwrap();
    let hub: Arc<dyn HfClientPort> = Arc::new(FakeHub::default());

    let core =
        CoreBootstrap::build_with_hub(minimal_config(&dir), noop_emitter(), Arc::clone(&hub))
            .await
            .expect("a core over a stand-in Hub builds");

    assert!(Arc::ptr_eq(&core.hf_client, &hub));
    assert!(core.repos.models.list().await.unwrap().is_empty());
}
