//! List command implementation for displaying stored GGUF models.
//!
//! This module handles retrieving and displaying all models from the
//! database in a formatted table with key metadata.

use crate::commands::presentation::{print_separator, truncate_string};
use crate::services::core::AppCore;
use anyhow::Result;
use std::sync::Arc;

/// Handles the "list" command to display all GGUF models in the database.
///
/// This function retrieves and displays all models stored in the database
/// with their metadata including name, file path, parameter count, architecture,
/// quantization, context length, and when they were added.
///
/// # Returns
///
/// Returns `Result<()>` indicating the success or failure of the operation.
///
/// # Errors
///
/// This function will return an error if:
/// - Database connection fails
/// - Database query fails
///
/// # Examples
///
/// ```rust,no_run
/// use gglib::commands::list::handle_list;
/// use gglib::services::AppCore;
/// use std::sync::Arc;
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     let pool = gglib::services::database::setup_database().await?;
///     let core = Arc::new(AppCore::new(pool));
///     handle_list(core).await?;
///     Ok(())
/// }
/// ```
pub async fn handle_list(core: Arc<AppCore>) -> Result<()> {
    // Retrieve all models via AppCore
    let models = core.models().list().await?;

    if models.is_empty() {
        println!("No models found in the database.");
        println!("Use 'gglib add <file_path>' to add your first model.");
        return Ok(());
    }

    println!("Found {} model(s) in the database:\n", models.len());

    // Display models in a formatted table with enhanced metadata
    println!(
        "{:<3} {:<25} {:<8} {:<12} {:<8} {:<10} {:<20} File Path",
        "ID", "Name", "Params", "Arch", "Quant", "Context", "Added"
    );
    print_separator(115);

    for model in models {
        let arch = model.architecture.as_deref().unwrap_or("--");
        let quant = model.quantization.as_deref().unwrap_or("--");
        let context = model
            .context_length
            .map(|c| c.to_string())
            .unwrap_or_else(|| "--".to_string());

        println!(
            "{:<3} {:<25} {:<8.1} {:<12} {:<8} {:<10} {:<20} {}",
            model
                .id
                .map(|id| id.to_string())
                .unwrap_or_else(|| "--".to_string()),
            truncate_string(&model.name, 24),
            model.param_count_b,
            truncate_string(arch, 11),
            truncate_string(quant, 7),
            truncate_string(&context, 9),
            model.added_at.format("%Y-%m-%d %H:%M:%S"),
            model.file_path.display()
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_string_no_truncation_needed() {
        let result = truncate_string("short", 10);
        assert_eq!(result, "short");
    }

    #[test]
    fn test_truncate_string_exact_length() {
        let result = truncate_string("exactly10c", 10);
        assert_eq!(result, "exactly10c");
    }

    #[test]
    fn test_truncate_string_needs_truncation() {
        let result = truncate_string("this is a very long string", 10);
        assert_eq!(result, "this is...");
    }

    #[test]
    fn test_truncate_string_very_short_limit() {
        let result = truncate_string("hello", 3);
        assert_eq!(result, "...");
    }

    #[test]
    fn test_truncate_string_zero_limit() {
        let result = truncate_string("hello", 0);
        assert_eq!(result, "...");
    }

    #[test]
    fn test_truncate_string_empty_input() {
        let result = truncate_string("", 10);
        assert_eq!(result, "");
    }

    // Integration tests for handle_list would go in tests/ directory
    // since they require database setup and mocking
}
