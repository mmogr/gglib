//! Add command handler.
//!
//! Handles adding a new GGUF model to the database by validating
//! the file, extracting metadata, prompting for missing info, and saving.

use anyhow::Result;
use std::path::PathBuf;

use crate::bootstrap::CliContext;
use crate::presentation::{ModelSummaryOpts, display_model_summary};
use crate::utils::input;

use gglib_core::domain::{NameSource, resolve_model_name};
use gglib_core::services::ImportMode;
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
/// * `force` - Re-import a file already in the library, overwriting its row
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
/// - The file is already in the library and `force` was not passed
/// - Database operations fail
pub(crate) async fn execute(ctx: &CliContext, file_path: &str, force: bool) -> Result<()> {
    let path = PathBuf::from(file_path);

    // Validate the GGUF file and extract metadata for CLI preview
    let gguf_metadata = validation::validate_and_parse_gguf(ctx.gguf_parser.as_ref(), file_path)?;
    println!("File validation and metadata extraction successful.");

    // Refuse a duplicate here rather than after the prompts below. The core
    // import checks too — that is the guard that actually protects the
    // database — but reaching it costs the user a parameter-count prompt
    // first, and answering questions about a model only to be told it was
    // already there reads as a bug even though the refusal is correct.
    if !force && let Some(existing) = ctx.app.models().find_by_path(&path).await? {
        anyhow::bail!(
            "'{}' is already in the library as \"{}\" (id {}).\n\
             Pass --force to re-import it and refresh its derived metadata.",
            path.display(),
            existing.name,
            existing.id
        );
    }

    // Display extracted metadata to the user
    println!("\nExtracted metadata:");
    let resolved_name = resolve_model_name(Some(&gguf_metadata), &path, NameSource::LocalFile);
    println!("  Name: {resolved_name}");
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

    // Prompt for parameter count override (CLI-specific interactive UX).
    //
    // Skipped entirely under --force. `param_count_b` is not among the columns
    // the upsert refreshes, so on this path the answer could only be collected
    // and then thrown away — the very thing moving the duplicate check above
    // the prompts was meant to stop.
    let param_count_override = if force {
        println!(
            "\nSkipping the parameter-count prompt: --force refreshes derived \
             metadata only and leaves the stored parameter count alone."
        );
        None
    } else if let Some(params) = gguf_metadata.param_count_b {
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
    let mode = if force {
        ImportMode::Refresh
    } else {
        ImportMode::Fresh
    };
    let saved_model = ctx
        .app
        .models()
        .import_from_file(&path, ctx.gguf_parser.as_ref(), param_count_override, mode)
        .await?;

    // Display clean summary using shared presentation
    if force {
        println!("\nRe-derived from file:");
    } else {
        println!("\nModel successfully created:");
    }
    display_model_summary(&saved_model, ModelSummaryOpts::with_title(""));

    if force {
        // Be precise about what moved. The row is re-read after the upsert, so
        // the name shown above is the stored one — announcing a blanket
        // "refreshed" over it would tell the user something the database did
        // not do.
        println!("Replaced: tags, capabilities, dialect spec.");
        println!("Updated where newly detected: quantization, context length, expert counts.");
        println!("Unchanged: name, parameter count, architecture.");
    } else {
        println!("Model successfully added to database!");
    }
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
