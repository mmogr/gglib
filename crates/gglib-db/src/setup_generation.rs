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
//! This is a number of its own, kept in its own table, rather than
//! `PRAGMA user_version`: that ladder gates one-time row passes and
//! deliberately says nothing of columns (see `add_column_if_missing`).
//!
//! [`SCHEMA_GENERATION`] is kept honest by `schema_snapshot.sql`: the
//! schema a fresh database ends up with, which a test compares against. A
//! change to the schema fails that test until the generation is raised and
//! the snapshot rewritten.

use anyhow::{Result, bail};
use sqlx::SqlitePool;

/// The generation of the schema [`super::create_schema`] builds.
///
/// Raise it by one whenever that schema changes in any way: a table, a
/// column, an index, or a new step on the `user_version` ladder. Then
/// rewrite the snapshot with
/// `GGLIB_UPDATE_SCHEMA_SNAPSHOT=1 cargo test -p gglib-db schema_snapshot`.
pub(super) const SCHEMA_GENERATION: i64 = 2;

/// The generation stamped on the file `pool` opens, or `0` for a file no
/// build has stamped: a new file, or one from before the generation existed.
async fn stored(pool: &SqlitePool) -> Result<i64> {
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

/// Refuse a database a newer build has written. Called before anything
/// creates or alters a table.
pub(super) async fn refuse_newer(pool: &SqlitePool) -> Result<()> {
    let found = stored(pool).await?;
    if found > SCHEMA_GENERATION {
        bail!(
            "this gglib library was last opened by a newer gglib (schema generation {found}; \
             this build knows {SCHEMA_GENERATION}). Run the newer gglib, or upgrade this one. \
             To keep a development build's library apart from your installed one, set \
             GGLIB_DATA_DIR and GGLIB_DAEMON_PORT for it."
        );
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
