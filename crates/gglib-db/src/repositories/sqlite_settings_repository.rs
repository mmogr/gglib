//! `SQLite` implementation of the `SettingsRepository` trait.

use std::collections::BTreeMap;

use async_trait::async_trait;
use serde_json::Value;
use sqlx::{Row, SqliteConnection, SqlitePool};

use gglib_core::ports::SettingsChange;
use gglib_core::{CoreError, RepositoryError, Settings, SettingsRepository};

/// `SQLite` implementation of the `SettingsRepository` trait.
///
/// Stores each setting as an individual row in the key-value table, using the
/// `serde` field name as the key and a compact JSON encoding as the value.
/// `None`-valued fields are not stored; an absent row means "use default".
pub struct SqliteSettingsRepository {
    pool: SqlitePool,
}

impl SqliteSettingsRepository {
    /// Create a new `SQLite` settings repository.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Ensure the settings table exists.
    ///
    /// Call this during initialization to set up the schema.
    pub async fn ensure_table(&self) -> Result<(), RepositoryError> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS settings_kv (
                key TEXT PRIMARY KEY NOT NULL,
                value TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(storage)?;

        Ok(())
    }
}

#[async_trait]
impl SettingsRepository for SqliteSettingsRepository {
    async fn load(&self) -> Result<Settings, RepositoryError> {
        let mut conn = self.pool.acquire().await.map_err(storage)?;
        read(&mut conn).await
    }

    async fn save(&self, settings: &Settings) -> Result<(), RepositoryError> {
        let rows = rows_of(settings)?;
        let updated_at = now();
        let mut tx = self.pool.begin().await.map_err(storage)?;
        for (key, value) in &rows {
            write_row(&mut tx, key, value.as_ref(), &updated_at).await?;
        }
        tx.commit().await.map_err(storage)
    }

    /// One `BEGIN IMMEDIATE` transaction around the read, the change and the
    /// write.
    ///
    /// `IMMEDIATE`, not the deferred `BEGIN` `sqlx` issues by default. A
    /// deferred transaction takes the write lock at its first write, so two
    /// of them can read the same record and then both write it, and the
    /// second commit is the lost update this method exists to prevent. An
    /// immediate one takes the lock before it reads, so a second writer, from
    /// this pool or another process's, waits for the first to commit and
    /// then reads what it wrote. How long it waits is the busy timeout
    /// `setup_database` sets.
    ///
    /// Only the rows whose value changed are written. The lock is what makes
    /// this correct; writing less keeps a change to one field from rewriting
    /// every other field's row.
    async fn modify(&self, change: &SettingsChange<'_>) -> Result<Settings, CoreError> {
        let mut tx = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(storage)?;
        let before = read(&mut tx).await?;
        let mut after = before.clone();
        // A refusal returns with the transaction open, and dropping it rolls
        // back: nothing is stored.
        change(&mut after)?;
        let (was, is) = (rows_of(&before)?, rows_of(&after)?);
        let updated_at = now();
        for (key, value) in is
            .iter()
            .filter(|(key, value)| was.get(*key) != Some(*value))
        {
            write_row(&mut tx, key, value.as_ref(), &updated_at).await?;
        }
        tx.commit().await.map_err(storage)?;
        Ok(after)
    }
}

/// Every field of `settings` by its `serde` name: `Some` with the value its
/// row holds, or `None` for a field that is unset and has no row.
fn rows_of(settings: &Settings) -> Result<BTreeMap<String, Option<Value>>, RepositoryError> {
    match serde_json::to_value(settings).map_err(storage)? {
        Value::Object(map) => Ok(map
            .into_iter()
            .map(|(key, value)| (key, (!value.is_null()).then_some(value)))
            .collect()),
        other => Err(RepositoryError::Storage(format!(
            "expected object, got {other}"
        ))),
    }
}

/// The stored settings, read over `conn`.
async fn read(conn: &mut SqliteConnection) -> Result<Settings, RepositoryError> {
    let rows = sqlx::query("SELECT key, value FROM settings_kv")
        .fetch_all(&mut *conn)
        .await
        .map_err(storage)?;

    let mut map = serde_json::Map::new();
    for row in rows {
        let key: String = row.get("key");
        let raw: String = row.get("value");
        map.insert(key, serde_json::from_str::<Value>(&raw).map_err(storage)?);
    }

    serde_json::from_value(Value::Object(map)).map_err(storage)
}

/// Store `value` as `key`'s row, or remove the row when there is no value.
async fn write_row(
    conn: &mut SqliteConnection,
    key: &str,
    value: Option<&Value>,
    updated_at: &str,
) -> Result<(), RepositoryError> {
    let query = match value {
        Some(value) => sqlx::query(
            "INSERT OR REPLACE INTO settings_kv (key, value, updated_at) VALUES (?, ?, ?)",
        )
        .bind(key)
        .bind(value.to_string())
        .bind(updated_at),
        None => sqlx::query("DELETE FROM settings_kv WHERE key = ?").bind(key),
    };
    query.execute(conn).await.map(drop).map_err(storage)
}

fn now() -> String {
    chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

fn storage(e: impl std::fmt::Display) -> RepositoryError {
    RepositoryError::Storage(e.to_string())
}

#[cfg(test)]
#[path = "sqlite_settings_repository_tests.rs"]
mod sqlite_settings_repository_tests;
