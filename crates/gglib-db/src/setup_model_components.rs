//! The `model_components` table: the files an image model draws with beside
//! its weights, one row per role.
//!
//! A `#[path]` child of `setup.rs`, whose `create_schema` creates it after
//! `models` and `model_files`. A row goes with its model (`ON DELETE
//! CASCADE`), and the index on `path` answers which models link a file.

use anyhow::Result;
use sqlx::SqlitePool;

/// Creates `model_components` and its index on `path`.
///
/// No backfill: no library from before this table holds a component.
pub(super) async fn create_model_components_table(pool: &SqlitePool) -> Result<()> {
    sqlx::query(
        r"
        CREATE TABLE IF NOT EXISTS model_components (
            model_id INTEGER NOT NULL REFERENCES models(id) ON DELETE CASCADE,
            role TEXT NOT NULL,
            path TEXT NOT NULL,
            PRIMARY KEY (model_id, role)
        )
        ",
    )
    .execute(pool)
    .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_model_components_path ON model_components(path)")
        .execute(pool)
        .await?;
    Ok(())
}
