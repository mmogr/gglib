mod common;

use chrono::Utc;
use gglib_core::NewModel;
use gglib_core::settings::Settings;
use tempfile::TempDir;

use common::build_core;

/// A model inserted via the model repository can be retrieved by listing.
#[tokio::test]
async fn model_insert_and_list_round_trip() {
    let dir = TempDir::new().unwrap();
    let core = build_core(&dir).await;

    let model = NewModel::new(
        "My Model".to_string(),
        dir.path().join("model.gguf"),
        7.0,
        Utc::now(),
    );
    core.repos.models.insert(&model).await.unwrap();

    let models = core.repos.models.list().await.unwrap();
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].name, "My Model");
}

/// Settings written with `save` are returned verbatim by a subsequent `load`.
#[tokio::test]
async fn settings_save_and_reload_round_trip() {
    let dir = TempDir::new().unwrap();
    let core = build_core(&dir).await;

    let settings = Settings {
        proxy_port: Some(9999),
        ..Default::default()
    };
    core.repos.settings.save(&settings).await.unwrap();

    let loaded = core.repos.settings.load().await.unwrap();
    assert_eq!(loaded.proxy_port, Some(9999));
}

/// Immediately after bootstrapping the download queue is empty.
#[tokio::test]
async fn download_queue_starts_empty() {
    let dir = TempDir::new().unwrap();
    let core = build_core(&dir).await;
    assert_eq!(core.downloads.active_count().await.unwrap(), 0);
}

/// Immediately after bootstrapping there are no chat conversations.
#[tokio::test]
async fn chat_history_starts_empty() {
    let dir = TempDir::new().unwrap();
    let core = build_core(&dir).await;
    assert_eq!(core.repos.chat_history.get_conversation_count().await.unwrap(), 0);
}

/// Immediately after bootstrapping there are no MCP servers registered.
#[tokio::test]
async fn mcp_servers_start_empty() {
    let dir = TempDir::new().unwrap();
    let core = build_core(&dir).await;
    assert!(core.repos.mcp_servers.list().await.unwrap().is_empty());
}
