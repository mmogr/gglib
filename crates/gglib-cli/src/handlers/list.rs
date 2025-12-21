//! List command handler.
//!
//! Displays all GGUF models in the database in a formatted table.

use anyhow::Result;

use crate::bootstrap::CliContext;
use crate::presentation::{print_separator, truncate_string};

/// Execute the list command.
///
/// Retrieves and displays all models stored in the database
/// with their metadata including name, file path, parameter count,
/// architecture, quantization, context length, and when they were added.
///
/// # Arguments
///
/// * `ctx` - The CLI context providing access to AppCore
///
/// # Returns
///
/// Returns `Result<()>` indicating the success or failure of the operation.
///
/// # Errors
///
/// This function will return an error if:
/// - Database query fails
pub async fn execute(ctx: &CliContext) -> Result<()> {
    // Retrieve all models via AppCore
    let models = ctx.app().models().list().await?;

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
            model.id,
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
}
