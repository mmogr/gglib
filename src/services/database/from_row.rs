//! Custom FromRow implementation for the Gguf model.
//!
//! This module provides the SQLx FromRow trait implementation for deserializing
//! database rows into Gguf structs.

use chrono::{DateTime, NaiveDateTime, Utc};
use sqlx::Row;

/// Helper to parse datetime strings that may have "UTC" suffix.
fn parse_datetime(
    row: &sqlx::sqlite::SqliteRow,
    column: &str,
) -> Result<Option<DateTime<Utc>>, sqlx::Error> {
    let datetime_str: Option<String> = row.try_get(column)?;
    match datetime_str {
        Some(s) => {
            // Remove " UTC" suffix if present and parse
            let trimmed = s.trim_end_matches(" UTC");
            NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%d %H:%M:%S%.f")
                .map(|dt| Some(DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc)))
                .map_err(|e| sqlx::Error::Decode(Box::new(e)))
        }
        None => Ok(None),
    }
}

impl<'r> sqlx::FromRow<'r, sqlx::sqlite::SqliteRow> for crate::models::Gguf {
    fn from_row(row: &'r sqlx::sqlite::SqliteRow) -> Result<Self, sqlx::Error> {
        // Helper to convert i64 to u64 for context_length
        let context_length: Option<u64> = row
            .try_get::<Option<i64>, _>("context_length")?
            .map(|v| v as u64);

        Ok(crate::models::Gguf {
            id: row.try_get::<Option<i32>, _>("id")?.map(|v| v as u32),
            name: row.try_get("name")?,
            file_path: row.try_get::<String, _>("file_path")?.into(),
            param_count_b: row.try_get("param_count_b")?,
            architecture: row.try_get("architecture")?,
            quantization: row.try_get("quantization")?,
            context_length,
            metadata: serde_json::from_str(&row.try_get::<String, _>("metadata")?)
                .unwrap_or_default(),
            added_at: parse_datetime(row, "added_at")?.unwrap_or_else(Utc::now),
            hf_repo_id: row.try_get("hf_repo_id")?,
            hf_commit_sha: row.try_get("hf_commit_sha")?,
            hf_filename: row.try_get("hf_filename")?,
            download_date: parse_datetime(row, "download_date")?,
            last_update_check: parse_datetime(row, "last_update_check")?,
            tags: serde_json::from_str(&row.try_get::<String, _>("tags")?).unwrap_or_default(),
        })
    }
}
