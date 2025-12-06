//! Model CRUD operations for the database.
//!
//! This module provides functions for creating, reading, updating, and deleting
//! GGUF model records in the database.

use crate::models::Gguf;
use anyhow::Result;
use sqlx::{Row, SqlitePool};
use std::path::Path;

use super::error::ModelStoreError;

/// Shared SELECT column list for model queries.
/// This eliminates duplication across query functions.
pub(crate) const MODEL_SELECT_COLUMNS: &str = "id, name, file_path, param_count_b, architecture, quantization, context_length, metadata, added_at, hf_repo_id, hf_commit_sha, hf_filename, download_date, last_update_check, tags";

/// Internal record for duplicate checking.
struct ExistingModelRecord {
    id: u32,
    name: String,
    file_path: String,
}

/// Normalizes a file path to a canonical string representation.
fn normalized_file_path_string(path: &Path) -> String {
    std::fs::canonicalize(path)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| path.to_string_lossy().to_string())
}

/// Finds an existing model by its file path.
async fn find_existing_model_by_path(
    pool: &SqlitePool,
    file_path: &str,
) -> Result<Option<ExistingModelRecord>> {
    if let Some(row) =
        sqlx::query("SELECT id, name, file_path FROM models WHERE file_path = ? LIMIT 1")
            .bind(file_path)
            .fetch_optional(pool)
            .await?
    {
        let id: i64 = row.get("id");
        let name: String = row.get("name");
        let path: String = row.get("file_path");
        return Ok(Some(ExistingModelRecord {
            id: id as u32,
            name,
            file_path: path,
        }));
    }

    Ok(None)
}

/// Adds a new GGUF model to the database.
///
/// This function inserts a model record into the SQLite database with all
/// the model's metadata including name, file path, parameter count,
/// architecture, quantization, context length, and additional metadata.
///
/// # Arguments
///
/// * `pool` - A reference to the SQLite connection pool
/// * `model` - A reference to the `Gguf` model to be added to the database
///
/// # Returns
///
/// Returns `Result<()>` indicating success or failure of the database insertion.
///
/// # Errors
///
/// This function will return an error if:
/// - A model with the same file path already exists (`ModelStoreError::DuplicateModel`)
/// - The database connection fails
/// - The SQL insertion query fails
pub async fn add_model(pool: &SqlitePool, model: &Gguf) -> Result<()> {
    // Serialize metadata HashMap to JSON string
    let metadata_json = serde_json::to_string(&model.metadata)?;
    let file_path_string = normalized_file_path_string(&model.file_path);

    if let Some(existing) = find_existing_model_by_path(pool, &file_path_string).await? {
        return Err(ModelStoreError::DuplicateModel {
            model_name: existing.name,
            file_path: existing.file_path,
            existing_id: existing.id,
        }
        .into());
    }

    // Serialize tags as JSON array
    let tags_json = serde_json::to_string(&model.tags).unwrap_or_else(|_| "[]".to_string());

    sqlx::query("INSERT INTO models (name, file_path, param_count_b, architecture, quantization, context_length, metadata, added_at, hf_repo_id, hf_commit_sha, hf_filename, download_date, last_update_check, tags) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
        .bind(&model.name)
        .bind(&file_path_string)
        .bind(model.param_count_b)
        .bind(&model.architecture)
        .bind(&model.quantization)
        .bind(model.context_length.map(|c| c as i64))
        .bind(&metadata_json)
        .bind(model.added_at.to_string())
        .bind(&model.hf_repo_id)
        .bind(&model.hf_commit_sha)
        .bind(&model.hf_filename)
        .bind(model.download_date.as_ref().map(|d| d.to_string()))
        .bind(model.last_update_check.as_ref().map(|d| d.to_string()))
        .bind(&tags_json)
        .execute(pool)
        .await?;

    Ok(())
}

/// Retrieves all GGUF models from the database.
///
/// Returns models ordered by `added_at` descending (newest first).
pub async fn list_models(pool: &SqlitePool) -> Result<Vec<Gguf>> {
    let query = format!(
        "SELECT {} FROM models ORDER BY added_at DESC",
        MODEL_SELECT_COLUMNS
    );
    let models = sqlx::query_as::<_, Gguf>(&query).fetch_all(pool).await?;

    Ok(models)
}

/// Find models by name (partial match for user convenience).
///
/// This function searches for models where the name contains the provided
/// search term (case-insensitive). This allows users to find models
/// without typing the exact full name.
pub async fn find_models_by_name(pool: &SqlitePool, name: &str) -> Result<Vec<Gguf>> {
    let query = format!(
        "SELECT {} FROM models WHERE name LIKE ? ORDER BY added_at DESC",
        MODEL_SELECT_COLUMNS
    );
    let models = sqlx::query_as::<_, Gguf>(&query)
        .bind(format!("%{name}%"))
        .fetch_all(pool)
        .await?;

    Ok(models)
}

/// Get a model from the database by ID.
///
/// Returns `Ok(Some(Gguf))` if the model is found, `Ok(None)` if not found,
/// or an error if the database operation fails.
pub async fn get_model_by_id(pool: &SqlitePool, id: u32) -> Result<Option<Gguf>> {
    let query = format!("SELECT {} FROM models WHERE id = ?", MODEL_SELECT_COLUMNS);
    let model = sqlx::query_as::<_, Gguf>(&query)
        .bind(id as i64)
        .fetch_optional(pool)
        .await?;

    Ok(model)
}

/// Find a model by its file path.
///
/// Returns `Ok(Some(Gguf))` if a model with the given path is found,
/// `Ok(None)` if not found.
pub async fn find_model_by_path(pool: &SqlitePool, file_path: &str) -> Result<Option<Gguf>> {
    let query = format!(
        "SELECT {} FROM models WHERE file_path = ?",
        MODEL_SELECT_COLUMNS
    );
    let model = sqlx::query_as::<_, Gguf>(&query)
        .bind(file_path)
        .fetch_optional(pool)
        .await?;

    Ok(model)
}

/// Find a model by identifier (name or ID).
///
/// First tries to find by exact name match, then falls back to ID lookup
/// if the identifier is numeric.
pub async fn find_model_by_identifier(pool: &SqlitePool, identifier: &str) -> Result<Option<Gguf>> {
    // First try to find by exact name match
    let query = format!("SELECT {} FROM models WHERE name = ?", MODEL_SELECT_COLUMNS);
    let exact_match = sqlx::query_as::<_, Gguf>(&query)
        .bind(identifier)
        .fetch_optional(pool)
        .await?;

    if let Some(model) = exact_match {
        return Ok(Some(model));
    }

    // If no exact match, try to find by ID if the identifier is numeric
    if let Ok(id) = identifier.parse::<u32>() {
        return get_model_by_id(pool, id).await;
    }

    Ok(None)
}

/// Find a model by name with case-insensitive exact match.
///
/// This function searches for a model where the name matches exactly
/// (ignoring case). Unlike `find_models_by_name`, this returns at most
/// one model and requires an exact match rather than partial/substring.
///
/// Used by ProcessManager for SingleSwap strategy when resolving model names.
pub async fn find_model_by_name_case_insensitive(
    pool: &SqlitePool,
    name: &str,
) -> Result<Option<Gguf>> {
    let query = format!(
        "SELECT {} FROM models WHERE LOWER(name) = LOWER(?) LIMIT 1",
        MODEL_SELECT_COLUMNS
    );
    let model = sqlx::query_as::<_, Gguf>(&query)
        .bind(name)
        .fetch_optional(pool)
        .await?;

    Ok(model)
}

/// Update a model in the database.
///
/// Updates an existing model record with new values.
/// All fields except the ID and added_at timestamp can be updated.
///
/// # Errors
///
/// Returns `ModelStoreError::NotFound` if no model with the given ID exists.
pub async fn update_model(pool: &SqlitePool, id: u32, model: &Gguf) -> Result<()> {
    // Serialize metadata HashMap to JSON string
    let metadata_json = serde_json::to_string(&model.metadata)?;

    let result = sqlx::query("UPDATE models SET name = ?, file_path = ?, param_count_b = ?, architecture = ?, quantization = ?, context_length = ?, metadata = ?, hf_repo_id = ?, hf_commit_sha = ?, hf_filename = ?, download_date = ?, last_update_check = ? WHERE id = ?")
        .bind(&model.name)
        .bind(model.file_path.to_string_lossy().as_ref())
        .bind(model.param_count_b)
        .bind(&model.architecture)
        .bind(&model.quantization)
        .bind(model.context_length.map(|c| c as i64))
        .bind(&metadata_json)
        .bind(&model.hf_repo_id)
        .bind(&model.hf_commit_sha)
        .bind(&model.hf_filename)
        .bind(model.download_date.as_ref().map(|dt| dt.to_string()))
        .bind(model.last_update_check.as_ref().map(|dt| dt.to_string()))
        .bind(id as i64)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(ModelStoreError::NotFound { id }.into());
    }

    Ok(())
}

/// Remove a model from the database by ID.
///
/// Only removes the database entry - the actual model file is left untouched on disk.
///
/// # Errors
///
/// Returns `ModelStoreError::NotFound` if no model with the given ID exists.
pub async fn remove_model_by_id(pool: &SqlitePool, id: u32) -> Result<()> {
    let result = sqlx::query("DELETE FROM models WHERE id = ?")
        .bind(id as i64)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(ModelStoreError::NotFound { id }.into());
    }

    Ok(())
}
