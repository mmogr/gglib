//! The one-time passes over a library's rows, numbered on `PRAGMA
//! user_version`: a file stamped `N` has had steps `1..=N`.
//!
//! A `#[path]` child of `setup.rs`. Each step runs once per file, in its own
//! `BEGIN IMMEDIATE` transaction with its stamp, so a failed step stamps
//! nothing and the next open runs it again. `create_schema` runs step 1
//! before the projector link, which reads the paths it rewrites, and the
//! rest last.
//!
//! To add step 9: append it to [`step`], raise [`LATEST`] and
//! `SCHEMA_GENERATION` (`setup_generation.rs`), and rewrite the schema
//! snapshot as that constant's doc says.

use anyhow::{Result, bail};
use sqlx::{SqliteConnection, SqliteExecutor, SqlitePool};

/// The highest step, which a new file is stamped with.
pub(super) const LATEST: i64 = 8;

/// The file's `PRAGMA user_version`: the last step it has had.
pub(super) async fn user_version(conn: impl SqliteExecutor<'_>) -> sqlx::Result<i64> {
    sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(conn)
        .await
}

/// Run each step above the file's version, up to `through`, in order.
///
/// The version is read before any lock is taken, so a file already there
/// never waits for the write lock.
pub(super) async fn run(pool: &SqlitePool, through: i64) -> Result<()> {
    let found = user_version(pool).await?;
    for version in found + 1..=through {
        run_one(pool, version).await?;
    }
    Ok(())
}

/// Step `version` and its stamp, in one `BEGIN IMMEDIATE` transaction.
///
/// The version is asked again once the lock is held, so of two processes
/// opening one file the second finds the step done, and a later stamp is not
/// lowered.
async fn run_one(pool: &SqlitePool, version: i64) -> Result<()> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    if user_version(&mut *tx).await? >= version {
        return Ok(());
    }
    step(version, &mut tx).await?;
    // Not bindable as a parameter: SQLite requires a literal here.
    sqlx::query(&format!("PRAGMA user_version = {version}"))
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

/// The pass that stamp `version` stands for.
async fn step(version: i64, conn: &mut SqliteConnection) -> Result<()> {
    match version {
        1 => {
            super::backfill_local_model_keys(conn).await?;
            super::backfill_shard_path_lists(conn).await
        }
        2 => super::settings::fold_loop_guard_switch_into_mode(conn).await,
        3 => super::recipes::drop_stored_reasoning_recipes(conn).await,
        4 => super::remote_grant::flag_listed_servers(conn).await,
        5 => super::unknown_params::forget_zero_param_counts(conn).await,
        6 => super::mcp_servers::rebuild_without_type(conn).await,
        7 => super::loop_guard::make_the_guard_a_switch(conn).await,
        8 => super::format_tags::strip_format_tags(conn).await,
        _ => bail!("no migration to user_version {version}"),
    }
}

#[cfg(test)]
#[path = "setup_migrations_tests.rs"]
mod tests;

#[cfg(all(test, unix))]
#[path = "setup_migrations_upgrade_tests.rs"]
mod upgrade_tests;
