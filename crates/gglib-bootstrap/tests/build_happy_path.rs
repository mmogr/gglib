mod common;

use chrono::Utc;
use gglib_core::NewModel;
use tempfile::TempDir;

use common::build_core;

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
