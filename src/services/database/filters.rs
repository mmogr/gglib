//! Filter options queries for the model library.
//!
//! This module provides functions for retrieving aggregate filter data
//! used by the GUI filter popover.

use anyhow::Result;
use sqlx::{Row, SqlitePool};
use std::collections::HashSet;

/// Filter options for the model library UI.
///
/// Contains aggregate data about available models for building
/// dynamic filter controls.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ModelFilterOptions {
    /// All distinct quantization types present in the library
    pub quantizations: Vec<String>,
    /// Minimum and maximum parameter counts (in billions)
    pub param_range: Option<RangeValues>,
    /// Minimum and maximum context lengths
    pub context_range: Option<RangeValues>,
}

/// A range of numeric values with min and max.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RangeValues {
    pub min: f64,
    pub max: f64,
}

/// Get filter options aggregated from all models in the database.
///
/// Returns distinct quantizations, parameter count range, and context length range
/// for use in the GUI filter popover.
///
/// # Returns
///
/// Returns `ModelFilterOptions` with:
/// - `quantizations`: Sorted list of unique quantization types (e.g., "Q4_K_M", "Q8_0")
/// - `param_range`: Min/max parameter counts in billions, or None if no models
/// - `context_range`: Min/max context lengths, or None if no models have context info
pub async fn get_model_filter_options(pool: &SqlitePool) -> Result<ModelFilterOptions> {
    // Get distinct quantizations
    let quant_rows = sqlx::query(
        "SELECT DISTINCT quantization FROM models WHERE quantization IS NOT NULL AND quantization != ''"
    )
    .fetch_all(pool)
    .await?;

    let mut quantizations: Vec<String> = quant_rows
        .iter()
        .map(|row| row.get::<String, _>("quantization"))
        .collect();
    quantizations.sort();

    // Get param count range
    let param_row = sqlx::query(
        "SELECT MIN(param_count_b) as min_params, MAX(param_count_b) as max_params FROM models"
    )
    .fetch_one(pool)
    .await?;

    let min_params: Option<f64> = param_row.get("min_params");
    let max_params: Option<f64> = param_row.get("max_params");

    let param_range = match (min_params, max_params) {
        (Some(min), Some(max)) => Some(RangeValues { min, max }),
        _ => None,
    };

    // Get context length range
    let context_row = sqlx::query(
        "SELECT MIN(context_length) as min_ctx, MAX(context_length) as max_ctx FROM models WHERE context_length IS NOT NULL"
    )
    .fetch_one(pool)
    .await?;

    let min_ctx: Option<i64> = context_row.get("min_ctx");
    let max_ctx: Option<i64> = context_row.get("max_ctx");

    let context_range = match (min_ctx, max_ctx) {
        (Some(min), Some(max)) => Some(RangeValues {
            min: min as f64,
            max: max as f64,
        }),
        _ => None,
    };

    Ok(ModelFilterOptions {
        quantizations,
        param_range,
        context_range,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::database::{add_model, create_schema};
    use crate::models::Gguf;
    use std::path::PathBuf;
    use chrono::Utc;
    use std::collections::HashMap;

    async fn setup_test_db() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        create_schema(&pool).await.unwrap();
        pool
    }

    fn create_test_model(name: &str, params: f64, quant: Option<&str>, ctx: Option<u64>) -> Gguf {
        Gguf {
            id: None,
            name: name.to_string(),
            file_path: PathBuf::from(format!("/tmp/{}.gguf", name)),
            param_count_b: params,
            architecture: Some("llama".to_string()),
            quantization: quant.map(|s| s.to_string()),
            context_length: ctx,
            metadata: HashMap::new(),
            added_at: Utc::now(),
            hf_repo_id: None,
            hf_commit_sha: None,
            hf_filename: None,
            download_date: None,
            last_update_check: None,
            tags: vec![],
        }
    }

    #[tokio::test]
    async fn test_get_model_filter_options_empty_db() {
        let pool = setup_test_db().await;
        let options = get_model_filter_options(&pool).await.unwrap();

        assert!(options.quantizations.is_empty());
        assert!(options.param_range.is_none());
        assert!(options.context_range.is_none());
    }

    #[tokio::test]
    async fn test_get_model_filter_options_with_models() {
        let pool = setup_test_db().await;

        // Add test models with various properties
        add_model(&pool, &create_test_model("model1", 7.0, Some("Q4_K_M"), Some(4096))).await.unwrap();
        add_model(&pool, &create_test_model("model2", 13.0, Some("Q8_0"), Some(8192))).await.unwrap();
        add_model(&pool, &create_test_model("model3", 70.0, Some("Q4_K_M"), Some(32768))).await.unwrap();
        add_model(&pool, &create_test_model("model4", 3.0, Some("F16"), None)).await.unwrap();

        let options = get_model_filter_options(&pool).await.unwrap();

        // Check quantizations (should be sorted)
        assert_eq!(options.quantizations, vec!["F16", "Q4_K_M", "Q8_0"]);

        // Check param range
        let param_range = options.param_range.unwrap();
        assert!((param_range.min - 3.0).abs() < 0.001);
        assert!((param_range.max - 70.0).abs() < 0.001);

        // Check context range (excludes model4 which has no context)
        let context_range = options.context_range.unwrap();
        assert!((context_range.min - 4096.0).abs() < 0.001);
        assert!((context_range.max - 32768.0).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_get_model_filter_options_single_model() {
        let pool = setup_test_db().await;
        add_model(&pool, &create_test_model("only_model", 7.0, Some("Q4_K_M"), Some(4096))).await.unwrap();

        let options = get_model_filter_options(&pool).await.unwrap();

        assert_eq!(options.quantizations, vec!["Q4_K_M"]);

        // With single model, min == max
        let param_range = options.param_range.unwrap();
        assert!((param_range.min - 7.0).abs() < 0.001);
        assert!((param_range.max - 7.0).abs() < 0.001);
    }
}
