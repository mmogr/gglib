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
use sqlx::SqliteConnection;

/// Step 5: every stored `0.0` count becomes NULL.
pub(super) async fn forget_zero_param_counts(conn: &mut SqliteConnection) -> Result<()> {
    let forgotten = sqlx::query("UPDATE models SET param_count_b = NULL WHERE param_count_b = 0")
        .execute(&mut *conn)
        .await?
        .rows_affected();
    // A saved agentic report carries the count it was run with, `0` for
    // the same unknown.
    sqlx::query(
        "UPDATE benchmark_agentic_results \
         SET report_json = json_set(report_json, '$.param_count_b', json('null')) \
         WHERE json_valid(report_json) AND json_extract(report_json, '$.param_count_b') = 0",
    )
    .execute(&mut *conn)
    .await?;
    if forgotten > 0 {
        tracing::info!(forgotten, "parameter counts stored as 0.0 are now unknown");
    }
    Ok(())
}

#[cfg(test)]
#[path = "setup_unknown_params_tests.rs"]
mod setup_unknown_params_tests;
