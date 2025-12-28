//! Add command handler.
//!
//! Handles adding a new GGUF model to the database by validating
//! the file, extracting metadata, prompting for missing info, and saving.

use anyhow::Result;

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
    // Validate the GGUF file and extract metadata using injected parser
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
    let capabilities = ctx.gguf_parser().detect_capabilities(&gguf_metadata);
    let auto_tags = capabilities.to_tags();

    // Infer model capabilities from chat template
    let template = gguf_metadata.metadata.get("tokenizer.chat_template");
    let model_capabilities =
        gglib_core::domain::infer_from_chat_template(template.map(String::as_str));

    // Create the new model instance using gglib_core types
    let new_model = gglib_core::NewModel {
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
        file_paths: None,
        capabilities: model_capabilities,
    };

    // Save to database via AppCore
    let saved_model = ctx.app().models().add(new_model).await?;

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
