//! The parameter counts a library stored as `0.0` because none could be
//! read, made unknown.
//!
//! A `#[path]` child of `setup.rs`. Until `param_count_b` could be NULL, a
//! model whose tensors, labels and name gave no count was stored with `0.0`,
//! which every reader then showed as a real count of none. No model has no
//! parameters, so every `0.0` is such a count, and becomes NULL once, in the
//! catalogue and in every saved agentic report. It runs after the `models`
//! rebuild, which is what lets the column hold a NULL.

use anyhow::Result;
use sqlx::{SqliteExecutor, SqlitePool};

/// `PRAGMA user_version` once the `0.0` counts are unknown.
///
/// The fifth version this project has assigned, above
/// [`REMOTE_GRANT_SCHEMA_VERSION`](super::remote_grant::REMOTE_GRANT_SCHEMA_VERSION),
/// which goes on meaning what it meant; anything later takes a higher number
/// still.
pub(super) const UNKNOWN_PARAMS_SCHEMA_VERSION: i64 = 5;

/// Migration: every stored `0.0` count becomes NULL, once per database.
///
/// Gated on `user_version` and run in one `BEGIN IMMEDIATE` transaction, as
/// the steps below it are, and asked again once the lock is held, so of two
/// processes the second finds the work done and a later stamp is not
/// lowered.
pub(super) async fn forget_zero_param_counts(pool: &SqlitePool) -> Result<()> {
    if user_version(pool).await? >= UNKNOWN_PARAMS_SCHEMA_VERSION {
        return Ok(());
    }
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    if user_version(&mut *tx).await? >= UNKNOWN_PARAMS_SCHEMA_VERSION {
        return Ok(());
    }
    let forgotten = sqlx::query("UPDATE models SET param_count_b = NULL WHERE param_count_b = 0")
        .execute(&mut *tx)
        .await?
        .rows_affected();
    // A saved agentic report carries the count it was run with, `0` for
    // the same unknown.
    sqlx::query(
        "UPDATE benchmark_agentic_results \
         SET report_json = json_set(report_json, '$.param_count_b', json('null')) \
         WHERE json_valid(report_json) AND json_extract(report_json, '$.param_count_b') = 0",
    )
    .execute(&mut *tx)
    .await?;
    // Not bindable as a parameter: SQLite requires a literal here.
    sqlx::query(&format!(
        "PRAGMA user_version = {UNKNOWN_PARAMS_SCHEMA_VERSION}"
    ))
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    if forgotten > 0 {
        tracing::info!(forgotten, "parameter counts stored as 0.0 are now unknown");
    }
    Ok(())
}

/// The database's `PRAGMA user_version`.
async fn user_version(conn: impl SqliteExecutor<'_>) -> sqlx::Result<i64> {
    sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(conn)
        .await
}

#[cfg(test)]
#[path = "setup_unknown_params_tests.rs"]
mod setup_unknown_params_tests;
