//! The schema generation: one number that says which build's schema a
//! database file carries, so an older build refuses a newer one.
//!
//! Every gglib build on a machine opens the same database, a packaged
//! install and one built from a checkout alike. A newer build adds columns,
//! tables and row passes that an older build knows nothing of, and an older
//! build that kept writing to such a file would break invariants the newer
//! one relies on. So a build stamps the generation it was written for, and a
//! build that finds a higher one stops before touching the file.
//!
//! One older library is refused too: one a build before generation 11 wrote
//! on Windows, which stored each path in the verbatim form `canonicalize`
//! gives there. The paths are not rewritten in place; the refusal says to
//! move the file aside and start a new library.
//!
//! This is a number of its own, kept in its own table, rather than
//! `PRAGMA user_version`: that ladder numbers the one-time row passes in
//! `setup_migrations.rs` and deliberately says nothing of columns (see
//! `add_column_if_missing`).
//!
//! [`SCHEMA_GENERATION`] is kept honest by `schema_snapshot.sql`: the
//! schema a fresh database ends up with, which a test compares against. A
//! change to the schema fails that test until the generation is raised and
//! the snapshot rewritten.

use std::path::Path;

use anyhow::{Result, bail};
use sqlx::SqlitePool;

/// The generation of the schema [`super::create_schema_reading`] builds.
///
/// Raise it by one whenever that schema changes in any way: a table, a
/// column, an index, or a new step in `setup_migrations.rs`, and when the
/// form of what a row stores changes, as 11 did. Then rewrite the snapshot
/// with `GGLIB_UPDATE_SCHEMA_SNAPSHOT=1 cargo test -p gglib-db schema_snapshot`.
pub const SCHEMA_GENERATION: i64 = 11;

/// The first generation whose builds store every path in the form
/// `gglib_core::paths::canonical` writes it. Builds before it stored a path
/// as Windows' `canonicalize` gave it, verbatim prefix and all.
const PLAIN_PATHS: i64 = 11;

/// How a column holds a path.
#[derive(Clone, Copy)]
enum Holds {
    /// The column is the path.
    Path,
    /// A JSON list of paths: a model's shards.
    PathList,
    /// A JSON object whose `folder` is a path: a chat's settings.
    Folder,
}

/// Each column that may hold a stored path: the paths a model, its shards,
/// projector and components are stored under, and a chat's folder. A table
/// or column the file does not have yet is skipped.
const STORED_PATHS: [(&str, &str, Holds); 6] = [
    ("models", "file_path", Holds::Path),
    ("models", "projector_path", Holds::Path),
    ("models", "file_paths_json", Holds::PathList),
    ("model_files", "file_path", Holds::Path),
    ("model_components", "path", Holds::Path),
    ("chat_conversations", "settings", Holds::Folder),
];

/// The paths in one stored `value`. JSON that does not parse, as an older
/// build or a hand may have left it, holds none.
fn paths_in(value: String, holds: Holds) -> Vec<String> {
    let parsed = || serde_json::from_str::<serde_json::Value>(&value).ok();
    match holds {
        Holds::Path => vec![value],
        Holds::PathList => parsed()
            .and_then(|v| match v {
                serde_json::Value::Array(items) => Some(items),
                _ => None,
            })
            .unwrap_or_default()
            .into_iter()
            .filter_map(|item| item.as_str().map(str::to_owned))
            .collect(),
        Holds::Folder => parsed()
            .and_then(|v| v.get("folder")?.as_str().map(str::to_owned))
            .into_iter()
            .collect(),
    }
}

/// The generation stamped on the file `pool` opens, or `0` for a file no
/// build has stamped: a new file, or one from before the generation existed.
pub(super) async fn stored(pool: &SqlitePool) -> Result<i64> {
    let has_table: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'schema_generation'",
    )
    .fetch_one(pool)
    .await?;
    if has_table == 0 {
        return Ok(0);
    }
    Ok(
        sqlx::query_scalar::<_, i64>("SELECT MAX(generation) FROM schema_generation")
            .fetch_optional(pool)
            .await?
            .unwrap_or(0),
    )
}

/// A library a newer build has written, which this build refuses.
///
/// It reads as the whole refusal. A caller that can do better than refuse,
/// as the CLI does when another gglib on the machine opens the library,
/// finds it in the error with `downcast_ref`.
#[derive(Debug)]
pub struct NewerLibrary {
    /// The generation the library is stamped with.
    pub found: i64,
    /// The backup this build opens and how to put it back, when there is
    /// one: a sentence that starts with a space.
    pub restore: Option<String>,
}

impl std::fmt::Display for NewerLibrary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (found, restore) = (self.found, self.restore.as_deref().unwrap_or_default());
        write!(
            f,
            "this gglib library was last opened by a newer gglib (schema generation {found}; \
             this build knows {SCHEMA_GENERATION}). Run the newer gglib, or upgrade this one. \
             To keep a development build's library apart from your installed one, set \
             GGLIB_DATA_DIR and GGLIB_DAEMON_PORT for it.{restore}"
        )
    }
}

impl std::error::Error for NewerLibrary {}

/// Refuse a database a newer build has written, with [`NewerLibrary`],
/// naming the backup from before it was upgraded when there is one. Called
/// before anything creates or alters a table.
pub(super) async fn refuse_newer(pool: &SqlitePool, db_path: &Path) -> Result<()> {
    let found = stored(pool).await?;
    if found > SCHEMA_GENERATION {
        let restore = super::backup::how_to_restore(db_path);
        return Err(NewerLibrary { found, restore }.into());
    }
    Ok(())
}

/// Restamps the library at `db_path` one generation below this build's, so
/// the next open upgrades it, for a test of what an upgrade does.
#[cfg(any(test, feature = "test-utils"))]
pub async fn restamp_older(db_path: &Path) -> Result<()> {
    let options = sqlx::sqlite::SqliteConnectOptions::new().filename(db_path);
    let pool = SqlitePool::connect_with(options).await?;
    sqlx::query("UPDATE schema_generation SET generation = ?")
        .bind(SCHEMA_GENERATION - 1)
        .execute(&pool)
        .await?;
    pool.close().await;
    Ok(())
}

/// Refuse a library an older build wrote that holds a path in Windows'
/// verbatim form. This build stores the plain form, so such a library would
/// hold one file under two strings; it is not rewritten in place, and gglib
/// makes a new one once it is moved aside. Called before anything creates or
/// alters a table, so a refused file is left exactly as it was.
pub(super) async fn refuse_verbatim_paths(pool: &SqlitePool, db_path: &Path) -> Result<()> {
    if stored(pool).await? >= PLAIN_PATHS {
        return Ok(());
    }
    for (table, column, holds) in STORED_PATHS {
        let present: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM pragma_table_info(?) WHERE name = ?")
                .bind(table)
                .bind(column)
                .fetch_one(pool)
                .await?;
        if present == 0 {
            continue;
        }
        // Names from the list above, never from the file.
        let query = format!("SELECT {column} FROM {table} WHERE {column} IS NOT NULL");
        let values: Vec<String> = sqlx::query_scalar(&query).fetch_all(pool).await?;
        let mut paths = values.into_iter().flat_map(|v| paths_in(v, holds));
        if let Some(held) = paths.find(|p| gglib_core::paths::is_verbatim(p)) {
            let db = db_path.display();
            bail!(
                "this gglib library was written by an older gglib on Windows, which stored \
                 paths such as {held} in a form this gglib no longer uses, and it is not \
                 converted. Nothing has been changed. Move {db} aside, \
                 and {db}-wal and {db}-shm with it if they are beside it, or delete all \
                 three, and start gglib again: it makes a new library, without the old \
                 one's chats, settings, tool servers, or benchmark, compare and tune \
                 history. Then add your models with `gglib model add <file>`; the model \
                 files themselves are not touched."
            );
        }
    }
    Ok(())
}

/// Stamp this build's generation, once the schema is up to it. A file
/// already stamped with it is left alone, so an ordinary open writes nothing.
pub(super) async fn stamp(pool: &SqlitePool) -> Result<()> {
    if stored(pool).await? == SCHEMA_GENERATION {
        return Ok(());
    }
    sqlx::query("CREATE TABLE IF NOT EXISTS schema_generation (generation INTEGER NOT NULL)")
        .execute(pool)
        .await?;
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM schema_generation")
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO schema_generation (generation) VALUES (?)")
        .bind(SCHEMA_GENERATION)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
#[path = "setup_generation_tests.rs"]
mod tests;
