//! The `model_files` table, and the `models.projector_path` column a library
//! from before it is given.
//!
//! A `#[path]` child of `setup.rs`. `create_schema` creates the table, then
//! adds the column, which reads the table.

use std::path::{Path, PathBuf};

use anyhow::Result;
use gglib_core::GgufFileRole;
use gglib_core::paths::canonical_model_path_string;
use sqlx::{Row, SqliteConnection, SqlitePool};

/// Creates `model_files`, the per-file rows a downloaded model is verified
/// and update-checked against, and its index.
pub(super) async fn create_model_files_table(pool: &SqlitePool) -> Result<()> {
    sqlx::query(
        r"
        CREATE TABLE IF NOT EXISTS model_files (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            model_id INTEGER NOT NULL,
            file_path TEXT NOT NULL,
            file_index INTEGER NOT NULL,
            expected_size INTEGER NOT NULL,
            hf_oid TEXT,
            last_verified_at TEXT,
            FOREIGN KEY (model_id) REFERENCES models(id) ON DELETE CASCADE,
            UNIQUE (model_id, file_path)
        )
        ",
    )
    .execute(pool)
    .await?;

    // Index on model_id for faster model_files lookups
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_model_files_model_id ON model_files(model_id)")
        .execute(pool)
        .await?;
    Ok(())
}

/// Adds `models.projector_path` to a library from before it, and links each
/// model there to the projector among its own files.
///
/// A download from before the column stored a repository's projector as one
/// more file of the model: a `model_files` row, an entry of `file_paths_json`,
/// or both. Such a model has the file and no link to it, so it launches
/// without `--mmproj`. The link is made here, once, in the transaction that
/// adds the column: a library that already has the column is left exactly as
/// it is, so a model its user has since unlinked stays unlinked.
///
/// Nothing but `projector_path` is written and no row is deleted. The
/// projector's `model_files` row stays, so verification and the update check
/// still cover the file.
///
/// Two processes opening one old library at once both get this far. `BEGIN
/// IMMEDIATE` lets one write at a time and the shape is asked again inside the
/// transaction, so the second finds the column there and changes nothing.
pub(super) async fn add_projector_column(pool: &SqlitePool) -> Result<()> {
    let mut conn = pool.acquire().await?;
    if has_projector_column(&mut conn).await? {
        return Ok(());
    }
    sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
    let outcome = add_and_link(&mut conn).await;
    let end = if outcome.is_ok() {
        "COMMIT"
    } else {
        "ROLLBACK"
    };
    if let Err(ending) = sqlx::query(end).execute(&mut *conn).await {
        // The transaction's state is unknown, so the connection does not go
        // back to the pool.
        drop(conn.detach());
        return Err(match outcome {
            Ok(_) => ending.into(),
            Err(error) => error.context(format!("and the rollback failed: {ending}")),
        });
    }
    let linked = outcome?;
    if linked > 0 {
        tracing::info!(linked, "linked models to the projector among their files");
    }
    Ok(())
}

/// Whether `models` has the `projector_path` column.
async fn has_projector_column(conn: &mut SqliteConnection) -> Result<bool> {
    let present: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pragma_table_info('models') WHERE name = 'projector_path'",
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok(present > 0)
}

/// The column and the links, inside the transaction `conn` holds. Answers how
/// many models were linked.
async fn add_and_link(conn: &mut SqliteConnection) -> Result<usize> {
    if has_projector_column(conn).await? {
        return Ok(0);
    }
    sqlx::query("ALTER TABLE models ADD COLUMN projector_path TEXT")
        .execute(&mut *conn)
        .await?;

    let models = sqlx::query("SELECT id, file_path, file_paths_json FROM models")
        .fetch_all(&mut *conn)
        .await?;
    let mut linked = 0_usize;
    for model in &models {
        let id: i64 = model.try_get("id")?;
        let weights = PathBuf::from(model.try_get::<String, _>("file_path")?);
        let listed: Option<String> = model.try_get("file_paths_json")?;
        let rows: Vec<String> = sqlx::query_scalar(
            "SELECT file_path FROM model_files WHERE model_id = ? ORDER BY file_index",
        )
        .bind(id)
        .fetch_all(&mut *conn)
        .await?;

        let Some(projector) = own_projector(&weights, listed.as_deref(), &rows) else {
            continue;
        };
        sqlx::query("UPDATE models SET projector_path = ? WHERE id = ?")
            .bind(canonical_model_path_string(&projector))
            .bind(id)
            .execute(&mut *conn)
            .await?;
        linked += 1;
    }
    Ok(linked)
}

/// The first of a model's own files that is named as a projector and is on
/// disk.
///
/// `listed` is the model's `file_paths_json`, absolute paths; a value that is
/// not a JSON list of strings adds nothing. `rows` are its `model_files`
/// names. [`GgufFileRole::projectors_among`] says which of the two lists'
/// files are projectors, the listed ones first. A file that is not there is
/// passed over: a link to it would fail a launch that succeeds without one.
fn own_projector(weights: &Path, listed: Option<&str>, rows: &[String]) -> Option<PathBuf> {
    let listed: Vec<String> = listed
        .and_then(|json| serde_json::from_str(json).ok())
        .unwrap_or_default();
    GgufFileRole::projectors_among(weights, listed.iter().chain(rows)).find(|file| file.is_file())
}

#[cfg(test)]
#[path = "setup_model_files_tests.rs"]
mod tests;
