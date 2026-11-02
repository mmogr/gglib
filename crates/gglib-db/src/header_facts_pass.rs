//! The daemon's one pass over the models added before a model kept the type
//! its header states and the bits per weight of its tensor data.
//!
//! The column those replaced was dropped and nothing carried over from it, so
//! such a model has none of the three facts a model now keeps. Its file is
//! read once to fill the two a header gives; its label stays empty, since
//! only a download knows what its uploader called it.

use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use gglib_core::ports::GgufParserPort;
use gglib_core::services::read_header_facts;
use serde_json::Value;
use sqlx::SqlitePool;

use crate::repositories::sqlite_settings_repository::{now, write_row};

/// The `settings_kv` row a completed pass leaves, so no later start reads a
/// file again. No field of the settings has this key, so their read ignores
/// it.
const READ_KEY: &str = "header_facts_read";

/// Read the header's facts of every model that has none of the three, once
/// per library, and answer how many were filled: `None` when an earlier
/// pass completed and this one did nothing.
///
/// Every file is read on the blocking pool, so a slow disk or a dead mount
/// holds only this pass and never the runtime; the daemon starts it beside
/// its start, not before it. A pass that fails or never ends leaves no mark,
/// and the next start tries again.
///
/// Only the two facts are written, and only to a row that still has neither,
/// so a change another process made to the row meanwhile is kept.
///
/// # Errors
///
/// When the library cannot be read or written, or a read of a file panics.
pub async fn fill_header_facts_once(
    pool: &SqlitePool,
    parser: Arc<dyn GgufParserPort>,
) -> Result<Option<usize>> {
    let done: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM settings_kv WHERE key = ?")
        .bind(READ_KEY)
        .fetch_one(pool)
        .await?;
    if done > 0 {
        return Ok(None);
    }
    let rows: Vec<(i64, String)> = sqlx::query_as(
        "SELECT id, file_path FROM models \
         WHERE variant_label IS NULL AND file_type IS NULL AND bits_per_weight IS NULL",
    )
    .fetch_all(pool)
    .await?;

    let mut filled = 0;
    for (id, path) in rows {
        let parser = Arc::clone(&parser);
        let read =
            tokio::task::spawn_blocking(move || read_header_facts(Path::new(&path), &*parser));
        let Some(facts) = read.await? else {
            continue;
        };
        let written = sqlx::query(
            "UPDATE models SET file_type = ?, bits_per_weight = ? \
             WHERE id = ? AND file_type IS NULL AND bits_per_weight IS NULL",
        )
        .bind(facts.file_type.map(|file_type| i64::from(file_type.0)))
        .bind(facts.bits_per_weight)
        .bind(id)
        .execute(pool)
        .await?;
        if written.rows_affected() > 0 {
            filled += 1;
        }
    }

    let mut conn = pool.acquire().await?;
    write_row(&mut conn, READ_KEY, Some(&Value::Bool(true)), &now()).await?;
    Ok(Some(filled))
}

#[cfg(test)]
#[path = "header_facts_pass_tests.rs"]
mod tests;
