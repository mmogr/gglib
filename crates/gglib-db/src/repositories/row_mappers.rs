//! Row mapping helpers for `SQLite` queries.

use chrono::{DateTime, NaiveDateTime, Utc};
use gglib_core::{Model, ModelCapabilities, RepositoryError};
use sqlx::Row;
use std::path::Path;

/// Shared SELECT column list for model queries.
pub const MODEL_SELECT_COLUMNS: &str = "id, name, file_path, param_count_b, architecture, quantization, context_length, expert_count, expert_used_count, expert_shared_count, metadata, added_at, hf_repo_id, hf_commit_sha, hf_filename, download_date, last_update_check, tags, capabilities, inference_defaults";

/// Helper to parse datetime strings that may have "UTC" suffix.
pub fn parse_datetime(datetime_str: Option<String>) -> Option<DateTime<Utc>> {
    datetime_str.and_then(|s| {
        let trimmed = s.trim_end_matches(" UTC");
        NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%d %H:%M:%S%.f")
            .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc))
            .ok()
    })
}

/// Parse a database row into a Model.
pub fn row_to_model(row: &sqlx::sqlite::SqliteRow) -> Result<Model, RepositoryError> {
    let context_length: Option<u64> = row
        .try_get::<Option<i64>, _>("context_length")
        .map_err(|e| RepositoryError::Storage(e.to_string()))?
        .map(|v| v as u64);

    let metadata_json: String = row
        .try_get("metadata")
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;

    let tags_json: String = row
        .try_get("tags")
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;

    let added_at_str: Option<String> = row
        .try_get("added_at")
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;

    let download_date_str: Option<String> = row
        .try_get("download_date")
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;

    let last_update_check_str: Option<String> = row
        .try_get("last_update_check")
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;

    Ok(Model {
        id: row
            .try_get::<i64, _>("id")
            .map_err(|e| RepositoryError::Storage(e.to_string()))?,
        name: row
            .try_get("name")
            .map_err(|e| RepositoryError::Storage(e.to_string()))?,
        file_path: row
            .try_get::<String, _>("file_path")
            .map_err(|e| RepositoryError::Storage(e.to_string()))?
            .into(),
        param_count_b: row
            .try_get("param_count_b")
            .map_err(|e| RepositoryError::Storage(e.to_string()))?,
        architecture: row
            .try_get("architecture")
            .map_err(|e| RepositoryError::Storage(e.to_string()))?,
        quantization: row
            .try_get("quantization")
            .map_err(|e| RepositoryError::Storage(e.to_string()))?,
        context_length,
        expert_count: row.try_get::<Option<u32>, _>("expert_count").ok().flatten(),
        expert_used_count: row
            .try_get::<Option<u32>, _>("expert_used_count")
            .ok()
            .flatten(),
        expert_shared_count: row
            .try_get::<Option<u32>, _>("expert_shared_count")
            .ok()
            .flatten(),
        metadata: serde_json::from_str(&metadata_json).unwrap_or_default(),
        added_at: parse_datetime(added_at_str).unwrap_or_else(Utc::now),
        hf_repo_id: row
            .try_get("hf_repo_id")
            .map_err(|e| RepositoryError::Storage(e.to_string()))?,
        hf_commit_sha: row
            .try_get("hf_commit_sha")
            .map_err(|e| RepositoryError::Storage(e.to_string()))?,
        hf_filename: row
            .try_get("hf_filename")
            .map_err(|e| RepositoryError::Storage(e.to_string()))?,
        download_date: parse_datetime(download_date_str),
        last_update_check: parse_datetime(last_update_check_str),
        tags: serde_json::from_str(&tags_json).unwrap_or_default(),
        capabilities: row
            .try_get::<u32, _>("capabilities")
            .ok()
            .map(ModelCapabilities::from_bits_truncate)
            .unwrap_or_default(),
        inference_defaults: row
            .try_get::<Option<String>, _>("inference_defaults")
            .ok()
            .flatten()
            .and_then(|json| serde_json::from_str(&json).ok()),
    })
}

/// Normalizes a file path to a canonical string representation.
pub fn normalized_file_path_string(path: &Path) -> String {
    std::fs::canonicalize(path)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| path.to_string_lossy().to_string())
}
