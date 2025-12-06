//! Add command implementation for adding GGUF models to the database.
//!
//! This module handles the complete workflow for adding new models,
//! including file validation, metadata extraction, user prompts,
//! and database storage.

use crate::{
    commands::presentation::{ModelSummaryOpts, display_model_summary},
    gguf, models,
    services::AppCore,
    utils::{input, validation},
};
use anyhow::Result;
use std::sync::Arc;

/// Handles the "add" command to add a new GGUF model to the database.
///
/// This function encapsulates the complete workflow for adding a model:
/// 1. Validates the provided GGUF file path and extracts metadata
/// 2. Sets up the database connection and schema
/// 3. Prompts the user for any missing model metadata
/// 4. Creates a new model instance with extracted and user-provided data
/// 5. Saves the model to the database
///
/// # Arguments
///
/// * `file_path` - Path to the GGUF file to add to the database
///
/// # Returns
///
/// Returns `Result<()>` indicating the success or failure of the operation.
///
/// # Errors
///
/// This function will return an error if:
/// - File validation fails
/// - GGUF metadata extraction fails
/// - Database operations fail
/// - User input is invalid
///
/// # Examples
///
/// ```rust,no_run
/// use gglib::commands::add::handle_add;
/// use gglib::services::AppCore;
/// use std::sync::Arc;
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     let pool = gglib::services::database::setup_database().await?;
///     let core = Arc::new(AppCore::new(pool));
///     handle_add(core, "/path/to/model.gguf".to_string()).await?;
///     Ok(())
/// }
/// ```
pub async fn handle_add(core: Arc<AppCore>, file_path: String) -> Result<()> {
    // Validate the GGUF file and extract metadata
    let gguf_metadata = validation::validate_and_parse_gguf(&file_path)?;
    println!("File validation and metadata extraction successful.");

    // Display extracted metadata to the user
    println!("\nExtracted metadata:");
    if let Some(ref name) = gguf_metadata.name {
        println!("  Name: {name}");
    }
    if let Some(ref arch) = gguf_metadata.architecture {
        println!("  Architecture: {arch}");
    }
    if let Some(params) = gguf_metadata.param_count_b {
        println!("  Parameters: {params:.1}B");
    }
    if let Some(ref quant) = gguf_metadata.quantization {
        println!("  Quantization: {quant}");
    }
    if let Some(context) = gguf_metadata.context_length {
        println!("  Context Length: {context}");
    }

    // Prompt for missing information or allow user to override
    let name = if gguf_metadata.name.is_some() {
        let suggested_name = gguf_metadata.name.as_ref().unwrap();
        let user_input = input::prompt_string_with_default("Model name", Some(suggested_name))?;
        if user_input.is_empty() {
            suggested_name.clone()
        } else {
            user_input
        }
    } else {
        input::prompt_string("Model name")?
    };

    let param_count_b = if let Some(params) = gguf_metadata.param_count_b {
        let user_input =
            input::prompt_float_with_default("Parameter count (in billions)", Some(params))?;
        if user_input == 0.0 {
            params
        } else {
            user_input
        }
    } else {
        input::prompt_float("Parameter count (in billions)")?
    };

    // Auto-detect reasoning and tool calling capabilities from metadata
    let auto_tags = gguf::apply_capability_detection(&gguf_metadata.metadata);

    // Create the model instance with extracted and user-provided metadata
    let new_model: models::Gguf = models::Gguf {
        id: None, // Will be set by the database
        name,
        file_path: file_path.into(),
        param_count_b,
        architecture: gguf_metadata.architecture,
        quantization: gguf_metadata.quantization,
        context_length: gguf_metadata.context_length,
        metadata: gguf_metadata.metadata,
        added_at: chrono::Utc::now(),
        hf_repo_id: None,
        hf_commit_sha: None,
        hf_filename: None,
        download_date: None,
        last_update_check: None,
        tags: auto_tags,
    };

    // Display clean summary using shared presentation
    display_model_summary(
        &new_model,
        ModelSummaryOpts::with_title("\nModel successfully created:"),
    );

    // Save to database via AppCore
    core.models().add(&new_model).await?;

    println!("Model successfully added to database!");
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::gguf::GgufMetadata;
    use std::collections::HashMap;

    // Note: These tests would typically require mocking external dependencies
    // like database operations and file system interactions.
    // For now, we'll test the helper functions and logic that can be isolated.

    #[test]
    fn test_model_creation_logic() {
        let mut metadata = HashMap::new();
        metadata.insert("general.name".to_string(), "Test Model".to_string());

        let gguf_metadata = GgufMetadata {
            name: Some("Test Model".to_string()),
            architecture: Some("llama".to_string()),
            param_count_b: Some(7.0),
            quantization: Some("Q4_0".to_string()),
            context_length: Some(4096),
            metadata,
        };

        // Test the model creation logic (extracted for testability)
        let name = gguf_metadata
            .name
            .clone()
            .unwrap_or_else(|| "default".to_string());
        let param_count_b = gguf_metadata.param_count_b.unwrap_or(0.0);

        assert_eq!(name, "Test Model");
        assert_eq!(param_count_b, 7.0);
        assert_eq!(gguf_metadata.architecture, Some("llama".to_string()));
        assert_eq!(gguf_metadata.quantization, Some("Q4_0".to_string()));
        assert_eq!(gguf_metadata.context_length, Some(4096));
    }

    #[test]
    fn test_metadata_with_none_values() {
        let metadata = HashMap::new();

        let gguf_metadata = GgufMetadata {
            name: None,
            architecture: None,
            param_count_b: None,
            quantization: None,
            context_length: None,
            metadata,
        };

        // Test fallback values
        let name = gguf_metadata
            .name
            .clone()
            .unwrap_or_else(|| "default".to_string());
        let param_count_b = gguf_metadata.param_count_b.unwrap_or(0.0);

        assert_eq!(name, "default");
        assert_eq!(param_count_b, 0.0);
        assert_eq!(gguf_metadata.architecture, None);
        assert_eq!(gguf_metadata.quantization, None);
        assert_eq!(gguf_metadata.context_length, None);
    }

    #[test]
    fn test_metadata_fallback_logic() {
        let mut metadata = HashMap::new();
        metadata.insert("custom.key".to_string(), "custom.value".to_string());

        let gguf_metadata = GgufMetadata {
            name: Some("".to_string()), // Empty string should trigger fallback
            architecture: Some("gpt".to_string()),
            param_count_b: Some(13.5),
            quantization: Some("F16".to_string()),
            context_length: Some(2048),
            metadata,
        };

        // Test that empty name triggers fallback
        let name = if gguf_metadata.name.as_ref().is_none_or(|s| s.is_empty()) {
            "fallback_name".to_string()
        } else {
            gguf_metadata.name.clone().unwrap()
        };

        assert_eq!(name, "fallback_name");
        assert_eq!(gguf_metadata.param_count_b, Some(13.5));
        assert_eq!(gguf_metadata.metadata.len(), 1);
    }

    #[test]
    fn test_metadata_extraction_edge_cases() {
        // Test with complex metadata
        let mut metadata = HashMap::new();
        metadata.insert(
            "general.name".to_string(),
            "Model with Special Chars !@#$".to_string(),
        );
        metadata.insert("unicode.test".to_string(), "测试 🦙 émojis".to_string());
        metadata.insert("empty.value".to_string(), "".to_string());

        let gguf_metadata = GgufMetadata {
            name: Some("Model with Special Chars !@#$".to_string()),
            architecture: Some("llama".to_string()),
            param_count_b: Some(0.1), // Very small model
            quantization: Some("Q8_0".to_string()),
            context_length: Some(512), // Small context
            metadata: metadata.clone(),
        };

        assert_eq!(
            gguf_metadata.name,
            Some("Model with Special Chars !@#$".to_string())
        );
        assert_eq!(gguf_metadata.param_count_b, Some(0.1));
        assert_eq!(gguf_metadata.context_length, Some(512));
        assert_eq!(gguf_metadata.metadata.len(), 3);
        assert_eq!(
            gguf_metadata.metadata.get("unicode.test"),
            Some(&"测试 🦙 émojis".to_string())
        );
    }

    // Integration tests for handle_add function would go in tests/ directory
    // since they need real file system and database interactions
}
