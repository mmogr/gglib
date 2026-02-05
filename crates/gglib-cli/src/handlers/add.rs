//! Add command handler.
//!
//! Handles adding a new GGUF model to the database by validating
//! the file, extracting metadata, prompting for missing info, and saving.

use anyhow::Result;
use std::path::PathBuf;

use crate::bootstrap::CliContext;
use crate::presentation::{ModelSummaryOpts, display_model_summary};
use crate::utils::input;

use gglib_core::utils::validation;

/// Execute the add command.
///
/// Validates the GGUF file, extracts metadata, prompts user for missing
/// information, and saves the model to the database.
///
/// # Arguments
///
/// * `ctx` - The CLI context providing access to AppCore and parser
/// * `file_path` - Path to the GGUF file to add
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
pub async fn execute(ctx: &CliContext, file_path: &str) -> Result<()> {
    let path = PathBuf::from(file_path);

    // Validate the GGUF file and extract metadata for CLI preview
    let gguf_metadata = validation::validate_and_parse_gguf(ctx.gguf_parser().as_ref(), file_path)?;
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

    // Prompt for parameter count override (CLI-specific interactive UX)
    let param_count_override = if let Some(params) = gguf_metadata.param_count_b {
        let user_input =
            input::prompt_float_with_default("Parameter count (in billions)", Some(params))?;
        if user_input == 0.0 {
            None
        } else {
            Some(user_input)
        }
    } else {
        Some(input::prompt_float("Parameter count (in billions)")?)
    };

    // Delegate to shared core logic for model import
    let saved_model = ctx
        .app()
        .models()
        .import_from_file(&path, ctx.gguf_parser().as_ref(), param_count_override)
        .await?;

    // Display clean summary using shared presentation
    println!("\nModel successfully created:");
    display_model_summary(&saved_model, ModelSummaryOpts::with_title(""));

    println!("Model successfully added to database!");
    Ok(())
}

#[cfg(test)]
mod tests {
    // Note: These tests would typically require mocking external dependencies
    // like database operations and file system interactions.
    // For now, we'll test the helper functions and logic that can be isolated.

    #[test]
    fn test_add_handler_exists() {
        // Placeholder test to ensure module compiles
    }
}
