//! Tag management operations for models.
//!
//! This module provides functions for adding, removing, and querying tags
//! associated with GGUF models.

use anyhow::Result;
use sqlx::{Row, SqlitePool};
use std::collections::HashSet;

/// Parse a JSON string of tags into a Vec<String>.
///
/// Returns an empty vector if parsing fails.
pub(crate) fn parse_tags_json(json: &str) -> Vec<String> {
    serde_json::from_str(json).unwrap_or_default()
}

/// Fetch the tags JSON for a specific model and parse it.
///
/// This is a helper to eliminate repeated query patterns.
async fn fetch_model_tags(pool: &SqlitePool, model_id: u32) -> Result<Vec<String>> {
    let row = sqlx::query("SELECT tags FROM models WHERE id = ?")
        .bind(model_id as i64)
        .fetch_one(pool)
        .await?;

    let tags_json: String = row.get("tags");
    Ok(parse_tags_json(&tags_json))
}

/// Update the tags for a specific model.
async fn update_model_tags(pool: &SqlitePool, model_id: u32, tags: &[String]) -> Result<()> {
    let tags_json = serde_json::to_string(tags)?;
    sqlx::query("UPDATE models SET tags = ? WHERE id = ?")
        .bind(tags_json)
        .bind(model_id as i64)
        .execute(pool)
        .await?;
    Ok(())
}

/// Get all unique tags used across all models.
///
/// Returns a sorted list of all tags in use.
pub async fn list_tags(pool: &SqlitePool) -> Result<Vec<String>> {
    let rows =
        sqlx::query("SELECT DISTINCT tags FROM models WHERE tags IS NOT NULL AND tags != '[]'")
            .fetch_all(pool)
            .await?;

    let mut all_tags = HashSet::new();
    for row in rows {
        let tags_json: String = row.get("tags");
        for tag in parse_tags_json(&tags_json) {
            all_tags.insert(tag);
        }
    }

    let mut tags: Vec<String> = all_tags.into_iter().collect();
    tags.sort();
    Ok(tags)
}

/// Add a tag to a model.
///
/// If the tag already exists on the model, this is a no-op.
pub async fn add_model_tag(pool: &SqlitePool, model_id: u32, tag: String) -> Result<()> {
    let mut tags = fetch_model_tags(pool, model_id).await?;

    // Add tag if not already present
    if !tags.contains(&tag) {
        tags.push(tag);
        tags.sort();
        update_model_tags(pool, model_id, &tags).await?;
    }

    Ok(())
}

/// Remove a tag from a model.
///
/// If the tag doesn't exist on the model, this is a no-op.
pub async fn remove_model_tag(pool: &SqlitePool, model_id: u32, tag: String) -> Result<()> {
    let mut tags = fetch_model_tags(pool, model_id).await?;

    // Remove tag
    tags.retain(|t| t != &tag);
    update_model_tags(pool, model_id, &tags).await?;

    Ok(())
}

/// Get all tags for a specific model.
pub async fn get_model_tags(pool: &SqlitePool, model_id: u32) -> Result<Vec<String>> {
    fetch_model_tags(pool, model_id).await
}

/// Get all model IDs that have a specific tag.
pub async fn get_models_by_tag(pool: &SqlitePool, tag: String) -> Result<Vec<u32>> {
    let rows = sqlx::query("SELECT id, tags FROM models")
        .fetch_all(pool)
        .await?;

    let mut model_ids = Vec::new();
    for row in rows {
        let tags_json: String = row.get("tags");
        let tags = parse_tags_json(&tags_json);
        if tags.contains(&tag) {
            model_ids.push(row.get::<i64, _>("id") as u32);
        }
    }

    Ok(model_ids)
}
