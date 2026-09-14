//! The `SQLite` settings repository against an in-memory database: what a
//! row holds, what is removed, and what a change writes.

use gglib_core::settings::SettingsError;

use super::*;

#[tokio::test]
async fn test_load_returns_all_none_when_empty() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    let repo = SqliteSettingsRepository::new(pool);
    repo.ensure_table().await.unwrap();

    // Empty table → all fields are None; application layer supplies defaults.
    let settings = repo.load().await.unwrap();
    assert_eq!(settings, Settings::default());
}

#[tokio::test]
async fn test_save_and_load() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    let repo = SqliteSettingsRepository::new(pool);
    repo.ensure_table().await.unwrap();

    let settings = Settings {
        default_context_size: Some(8192),
        proxy_port: Some(9090),
        ..Settings::with_defaults()
    };

    repo.save(&settings).await.unwrap();
    let loaded = repo.load().await.unwrap();

    assert_eq!(loaded.default_context_size, Some(8192));
    assert_eq!(loaded.proxy_port, Some(9090));
}

/// The settings table is key-value, so a new field needs no DDL — it
/// round-trips as its own row keyed by the serde field name.
#[tokio::test]
async fn test_save_and_load_bind_host() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    let repo = SqliteSettingsRepository::new(pool.clone());
    repo.ensure_table().await.unwrap();

    let settings = Settings {
        bind_host: Some("0.0.0.0".to_owned()),
        ..Settings::default()
    };
    repo.save(&settings).await.unwrap();

    assert_eq!(
        repo.load().await.unwrap().bind_host,
        Some("0.0.0.0".to_owned())
    );

    let row: Option<(String,)> =
        sqlx::query_as("SELECT value FROM settings_kv WHERE key = 'bind_host'")
            .fetch_optional(&pool)
            .await
            .unwrap();
    assert_eq!(
        row.map(|r| r.0),
        Some("\"0.0.0.0\"".to_owned()),
        "bind_host is stored under its serde field name as compact JSON"
    );
}

#[tokio::test]
async fn test_save_and_load_share_lan() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    let repo = SqliteSettingsRepository::new(pool);
    repo.ensure_table().await.unwrap();

    let mut settings = Settings {
        share_lan: Some(true),
        ..Settings::default()
    };
    repo.save(&settings).await.unwrap();
    assert_eq!(repo.load().await.unwrap().share_lan, Some(true));

    // `--share-lan false` must be persistable as a real value, not erased:
    // it is the documented way to switch LAN sharing back off.
    settings.share_lan = Some(false);
    repo.save(&settings).await.unwrap();
    assert_eq!(repo.load().await.unwrap().share_lan, Some(false));
}

/// A database written before these fields existed has no rows for them.
/// `Settings` is `#[serde(default)]` with `Option` fields, so the absent
/// rows deserialize to `None` rather than failing the whole load — which
/// is why no migration is needed for the new columns.
#[tokio::test]
async fn test_load_tolerates_rows_written_before_new_fields_existed() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    let repo = SqliteSettingsRepository::new(pool.clone());
    repo.ensure_table().await.unwrap();

    // Simulate a pre-existing DB: only an old key is present.
    sqlx::query(
        "INSERT INTO settings_kv (key, value, updated_at) VALUES ('proxy_port', '9090', '')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let loaded = repo.load().await.expect("older row sets still load");
    assert_eq!(loaded.proxy_port, Some(9090));
    assert_eq!(loaded.bind_host, None);
    assert_eq!(loaded.share_lan, None);
}

#[tokio::test]
async fn test_none_fields_are_deleted_from_db() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    let repo = SqliteSettingsRepository::new(pool.clone());
    repo.ensure_table().await.unwrap();

    // Save with a Some value — should create a row for proxy_port.
    let mut settings = Settings {
        proxy_port: Some(9090),
        ..Settings::default()
    };
    repo.save(&settings).await.unwrap();

    let row: Option<(String,)> =
        sqlx::query_as("SELECT value FROM settings_kv WHERE key = 'proxy_port'")
            .fetch_optional(&pool)
            .await
            .unwrap();
    assert!(
        row.is_some(),
        "proxy_port row should exist after saving Some"
    );

    // Now set that field to None and save again — row should be gone.
    settings.proxy_port = None;
    repo.save(&settings).await.unwrap();

    let row: Option<(String,)> =
        sqlx::query_as("SELECT value FROM settings_kv WHERE key = 'proxy_port'")
            .fetch_optional(&pool)
            .await
            .unwrap();
    assert!(
        row.is_none(),
        "proxy_port row should be deleted after saving None"
    );
}

/// A change to one field writes that field's row and no other.
///
/// The rows beside it carry an `updated_at` no write would produce, so a
/// rewrite of any of them, even to the value it already held, shows.
#[tokio::test]
async fn a_change_to_one_field_writes_only_its_row() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    let repo = SqliteSettingsRepository::new(pool.clone());
    repo.ensure_table().await.unwrap();
    repo.save(&Settings {
        proxy_port: Some(9090),
        bind_host: Some("127.0.0.1".to_owned()),
        ..Settings::default()
    })
    .await
    .unwrap();
    sqlx::query("UPDATE settings_kv SET updated_at = 'untouched'")
        .execute(&pool)
        .await
        .unwrap();

    repo.modify(&|settings: &mut Settings| {
        settings.proxy_port = Some(9191);
        Ok(())
    })
    .await
    .unwrap();

    let stamp = |key: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query_as::<_, (String,)>("SELECT updated_at FROM settings_kv WHERE key = ?")
                .bind(key)
                .fetch_one(&pool)
                .await
                .unwrap()
                .0
        }
    };
    assert_eq!(stamp("bind_host").await, "untouched");
    assert_ne!(stamp("proxy_port").await, "untouched");
    assert_eq!(repo.load().await.unwrap().proxy_port, Some(9191));
}

/// A change the closure refuses stores nothing, including what it set before
/// refusing.
#[tokio::test]
async fn a_refused_change_stores_nothing() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    let repo = SqliteSettingsRepository::new(pool);
    repo.ensure_table().await.unwrap();
    repo.save(&Settings {
        proxy_port: Some(9090),
        ..Settings::default()
    })
    .await
    .unwrap();

    let refused = repo
        .modify(&|settings: &mut Settings| {
            settings.proxy_port = Some(9191);
            Err(SettingsError::InvalidPort(1))
        })
        .await;

    assert!(
        matches!(refused, Err(CoreError::Settings(_))),
        "{refused:?}"
    );
    assert_eq!(repo.load().await.unwrap().proxy_port, Some(9090));
}

/// A field a change clears loses its row, as `save` removes one.
#[tokio::test]
async fn a_field_a_change_clears_loses_its_row() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    let repo = SqliteSettingsRepository::new(pool.clone());
    repo.ensure_table().await.unwrap();
    repo.save(&Settings {
        proxy_port: Some(9090),
        ..Settings::default()
    })
    .await
    .unwrap();

    repo.modify(&|settings: &mut Settings| {
        settings.proxy_port = None;
        Ok(())
    })
    .await
    .unwrap();

    let row: Option<(String,)> =
        sqlx::query_as("SELECT value FROM settings_kv WHERE key = 'proxy_port'")
            .fetch_optional(&pool)
            .await
            .unwrap();
    assert!(row.is_none(), "the cleared field's row is gone");
}
