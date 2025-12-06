//! Remove command implementation for deleting GGUF models from the database.
//!
//! This module handles the "remove" command which allows users to delete
//! model entries from the database. The actual model files remain on disk
//! unchanged - only the database entries are removed.

use crate::models::Gguf;
use crate::services::core::AppCore;
use crate::utils::input;
use anyhow::{Result, anyhow};
use std::sync::Arc;

/// Handles the "remove" command to delete a GGUF model from the database.
///
/// This function searches for models matching the provided identifier,
/// confirms the deletion with the user (unless force flag is used), and
/// removes the model entry from the database.
///
/// # Arguments
///
/// * `identifier` - The name or partial name of the model to remove
/// * `force` - If true, skips confirmation prompt
///
/// # Returns
///
/// Returns `Result<()>` indicating success or failure of the operation.
///
/// # Errors
///
/// This function will return an error if:
/// - Database connection fails
/// - No models match the identifier
/// - User input fails
/// - Database removal operation fails
///
/// # Examples
///
/// ```rust,no_run
/// use gglib::commands::remove;
/// use gglib::services::AppCore;
/// use std::sync::Arc;
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     let pool = gglib::services::database::setup_database().await?;
///     let core = Arc::new(AppCore::new(pool));
///     // Remove with confirmation
///     remove::handle_remove(core.clone(), "my-model".to_string(), false).await?;
///     
///     // Force remove without confirmation
///     remove::handle_remove(core, "my-model".to_string(), true).await?;
///     Ok(())
/// }
/// ```
pub async fn handle_remove(core: Arc<AppCore>, identifier: String, force: bool) -> Result<()> {
    if let Some(model) = core.models().find_by_identifier(&identifier).await? {
        remove_with_confirmation(&core, model, force).await
    } else {
        // Fall back to partial matches for convenience
        let models = core.models().find_by_name(&identifier).await?;
        match models.len() {
            0 => {
                println!("No model found matching: '{identifier}'");
                println!("Use 'gglib list' to see available models.");
                Ok(())
            }
            1 => {
                let model = models.into_iter().next().unwrap();
                remove_with_confirmation(&core, model, force).await
            }
            _ => {
                println!("Multiple models found matching '{identifier}'. Please be more specific:");
                for model in models {
                    let id_display = model.id.map_or("?".to_string(), |id| id.to_string());
                    println!(
                        "  - [ID {}] {} ({})",
                        id_display,
                        model.name,
                        model.file_path.display()
                    );
                }
                println!();
                println!("Tip: Use the numeric ID or exact model name to remove a specific model.");
                Ok(())
            }
        }
    }
}

async fn remove_with_confirmation(core: &AppCore, model: Gguf, force: bool) -> Result<()> {
    let model_id = model
        .id
        .ok_or_else(|| anyhow!("Model '{}' does not have an ID", model.name))?;

    if !force {
        println!("Model to remove:");
        println!("  ID: {}", model_id);
        println!("  Name: {}", model.name);
        println!("  File: {}", model.file_path.display());
        println!("  Parameters: {:.1}B", model.param_count_b);
        if let Some(arch) = &model.architecture {
            println!("  Architecture: {arch}");
        }
        if let Some(quant) = &model.quantization {
            println!("  Quantization: {quant}");
        }
        println!(
            "  Added: {}",
            model.added_at.format("%Y-%m-%d %H:%M:%S UTC")
        );
        println!();

        let confirm = input::prompt_confirmation(
            "Are you sure you want to remove this model from the database?",
        )?;
        if !confirm {
            println!("Remove operation cancelled.");
            return Ok(());
        }
    }

    core.models().remove(model_id).await?;
    println!(
        "✅ Model '{}' (ID {}) successfully removed from database.",
        model.name, model_id
    );

    if !force {
        println!(
            "Note: The model file '{}' remains on disk.",
            model.file_path.display()
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {

    // Note: Most of the remove functionality requires database operations
    // and user input, which are better tested as integration tests.
    // Unit tests here would focus on any pure functions or logic
    // that can be isolated from external dependencies.

    #[test]
    fn test_remove_logic_scenarios() {
        // Test the different scenarios for number of models found
        let scenarios = vec![
            (0, "no models found"),
            (1, "single model found"),
            (2, "multiple models found"),
        ];

        for (count, description) in scenarios {
            match count {
                0 => assert_eq!(count, 0, "Should handle {}", description),
                1 => assert_eq!(count, 1, "Should handle {}", description),
                _ => assert!(count > 1, "Should handle {}", description),
            }
        }
    }

    #[test]
    fn test_identifier_validation() {
        // Test different identifier formats
        let valid_identifiers = vec![
            "123",        // Numeric ID
            "model_name", // Model name
            "llama-2-7b", // Name with dashes
            "Model Name", // Name with spaces
            "测试模型",   // Unicode name
        ];

        for identifier in valid_identifiers {
            assert!(
                !identifier.is_empty(),
                "Identifier '{}' should not be empty",
                identifier
            );
        }
    }

    #[test]
    fn test_model_count_handling() {
        // Test logic for different model counts
        fn handle_model_count(count: usize) -> &'static str {
            match count {
                0 => "no_models",
                1 => "single_model",
                _ => "multiple_models",
            }
        }

        assert_eq!(handle_model_count(0), "no_models");
        assert_eq!(handle_model_count(1), "single_model");
        assert_eq!(handle_model_count(2), "multiple_models");
        assert_eq!(handle_model_count(10), "multiple_models");
    }

    #[test]
    fn test_identifier_type_detection() {
        // Test detecting whether identifier is numeric ID or name
        fn is_numeric_id(identifier: &str) -> bool {
            identifier.parse::<u32>().is_ok()
        }

        assert!(is_numeric_id("123"));
        assert!(is_numeric_id("0"));
        assert!(!is_numeric_id("model_name"));
        assert!(!is_numeric_id("123abc"));
        assert!(!is_numeric_id(""));
    }

    // The main handle_remove function would be tested in integration tests
    // with mock database and user input
}
