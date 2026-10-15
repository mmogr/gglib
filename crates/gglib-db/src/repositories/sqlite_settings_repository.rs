//! `SQLite` implementation of the `SettingsRepository` trait.

use std::collections::BTreeMap;

use async_trait::async_trait;
use serde_json::Value;
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqliteConnection, SqlitePool};

use gglib_core::ports::{SettingsChange, SettingsReset};
use gglib_core::settings::KEPT_ON_RESET;
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
    /// The one definition of the table. `create_schema` runs it once as the
    /// database opens; a table already there, and its rows, are left as they
    /// are. Private to the crate, so that call is the only one a build makes.
    pub(crate) async fn ensure_table(&self) -> Result<(), RepositoryError> {
        sqlx::query(
            r"
            CREATE TABLE IF NOT EXISTS settings_kv (
                key TEXT PRIMARY KEY NOT NULL,
                value TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )
            ",
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

    /// [`modify`](Self::modify)'s one transaction, over the rows this build
    /// can read, so a record a newer build wrote or one edited by hand does
    /// not stop the reset that would repair it.
    ///
    /// A row is readable when its value is JSON text that the field of its
    /// key accepts on its own, and the reset reads only those. A preference
    /// whose row is not readable is put back to its default, which for most
    /// is no row. A kept row ([`KEPT_ON_RESET`]) that is not readable is left
    /// as it was, since dropping the pairing or the roster locks the owner
    /// out of his devices.
    async fn reset_preferences(&self) -> Result<SettingsReset, CoreError> {
        let mut tx = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(storage)?;
        let rows = stored_rows(&mut tx).await.map_err(storage)?;
        let mut record = serde_json::Map::new();
        let mut unreadable = Vec::new();
        for row in &rows {
            // A key that is not text cannot be checked against the kept
            // rows, so the reset refuses rather than guess.
            let key: String = row.try_get("key").map_err(storage)?;
            if let Some(value) = readable_value(row, &key) {
                record.insert(key, value);
            } else {
                unreadable.push(key);
            }
        }
        let before: Settings = serde_json::from_value(Value::Object(record)).map_err(storage)?;
        let mut after = before.clone();
        after.reset_preferences();
        let (was, is) = (rows_of(&before)?, rows_of(&after)?);
        let (kept_unreadable, dropped): (Vec<String>, Vec<String>) = unreadable
            .into_iter()
            .partition(|key| KEPT_ON_RESET.contains(&key.as_str()));
        let updated_at = now();
        let unread = |key: &String| kept_unreadable.contains(key) || dropped.contains(key);
        for (key, value) in is
            .iter()
            .filter(|(key, value)| was.get(*key) != Some(*value) && !unread(key))
        {
            write_row(&mut tx, key, value.as_ref(), &updated_at).await?;
        }
        // A dropped row's key may name no field this build has, and then
        // its default is no row.
        for key in &dropped {
            let default = is.get(key).and_then(Option::as_ref);
            write_row(&mut tx, key, default, &updated_at).await?;
        }
        tx.commit().await.map_err(storage)?;
        Ok(SettingsReset {
            dropped,
            kept_unreadable,
        })
    }
}

/// `row`'s value, when it is JSON text that `key`'s field accepts on its
/// own; a key no field has accepts any JSON, as a whole record's read
/// ignores it.
fn readable_value(row: &SqliteRow, key: &str) -> Option<Value> {
    let raw: String = row.try_get("value").ok()?;
    let value: Value = serde_json::from_str(&raw).ok()?;
    let alone = serde_json::Map::from_iter([(key.to_owned(), value.clone())]);
    serde_json::from_value::<Settings>(Value::Object(alone)).ok()?;
    Some(value)
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
    let rows = stored_rows(conn).await.map_err(storage)?;
    let record = values(&rows).map_err(storage)?;
    serde_json::from_value(Value::Object(record)).map_err(storage)
}

/// Every stored row's `key` and `value`, read over `conn`.
pub(crate) async fn stored_rows(conn: &mut SqliteConnection) -> sqlx::Result<Vec<SqliteRow>> {
    sqlx::query("SELECT key, value FROM settings_kv")
        .fetch_all(conn)
        .await
}

/// Each row's value by its key, as the JSON it holds, or why this build
/// cannot read the rows: a key or a value that is not text, or a value that
/// is not JSON.
pub(crate) fn values(rows: &[SqliteRow]) -> anyhow::Result<serde_json::Map<String, Value>> {
    let mut record = serde_json::Map::new();
    for row in rows {
        let key: String = row.try_get("key")?;
        let raw: String = row.try_get("value")?;
        record.insert(key, serde_json::from_str::<Value>(&raw)?);
    }
    Ok(record)
}

/// Store `value` as `key`'s row, or remove the row when there is no value.
#[allow(
    clippy::option_if_let_else,
    reason = "grandfathered at lint inheritance, #1157"
)]
pub(crate) async fn write_row(
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

/// The time a row is written at, as its `updated_at` holds it.
pub(crate) fn now() -> String {
    chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

fn storage(e: impl std::fmt::Display) -> RepositoryError {
    RepositoryError::Storage(e.to_string())
}

#[cfg(test)]
#[path = "sqlite_settings_repository_tests.rs"]
mod sqlite_settings_repository_tests;

#[cfg(test)]
#[path = "sqlite_settings_table_tests.rs"]
mod sqlite_settings_table_tests;

#[cfg(test)]
#[path = "sqlite_settings_reset_tests.rs"]
mod sqlite_settings_reset_tests;
