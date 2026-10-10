//! Add command handler.
//!
//! Handles adding a new GGUF model to the database by validating
//! the file, extracting metadata, prompting for missing info, and saving
//! through `ModelOps::add`, the operation the app's add runs.

use anyhow::{Context as _, Result};
use std::path::PathBuf;

use crate::bootstrap::CliContext;
use crate::component_args::{ComponentArgs, request_components};
use crate::presentation::{ModelSummaryOpts, display_model_summary};
use crate::utils::input;

use gglib_app_services::ModelOps;
use gglib_app_services::types::{AddModelRequest, UpdateModelRequest};
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
/// * `ctx` - The CLI context providing access to `AppCore` and parser
/// * `ops` - The model operations the file is added through
/// * `file_path` - Path to the GGUF file to add
/// * `reimport` - Re-import a file already in the library, overwriting its row
/// * `components` - An image model's components, linked once it is added
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
/// - The file is already in the library and `reimport` was not passed
/// - Database operations fail
/// - A component is refused, after the model is added; the error says so
pub(crate) async fn execute(
    ctx: &CliContext,
    ops: &ModelOps,
    file_path: &str,
    reimport: bool,
    components: &ComponentArgs,
) -> Result<()> {
    let path = PathBuf::from(file_path);
    // A role named twice is refused before anything is asked or written.
    let components = request_components(&components.changes()?);

    // Validate the GGUF file and extract metadata for CLI preview
    let gguf_metadata = validation::validate_and_parse_gguf(ctx.gguf_parser.as_ref(), file_path)?;
    println!("File validation and metadata extraction successful.");

    // Refuse a duplicate here rather than after the prompts below. The
    // import checks too, under `ops.add` — that is the guard that actually
    // protects the database — but reaching it costs the user a
    // parameter-count prompt first, and answering questions about a model
    // only to be told it was already there reads as a bug even though the
    // refusal is correct.
    if !reimport && let Some(existing) = ctx.app.models().find_by_path(&path).await? {
        anyhow::bail!(
            "'{}' is already in the library as \"{}\" (id {}).\n\
             Pass --reimport to re-import it and refresh its derived metadata.",
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
    if let Some(family) = gguf_metadata.image_family {
        println!("  Draws: {} images", family.label());
    }

    // Prompt for parameter count override (CLI-specific interactive UX).
    //
    // Skipped entirely under --reimport. `param_count_b` is not among the columns
    // the upsert refreshes, so on this path the answer could only be collected
    // and then thrown away — the very thing moving the duplicate check above
    // the prompts was meant to stop.
    let param_count_override = if reimport {
        println!(
            "\nSkipping the parameter-count prompt: --reimport refreshes derived \
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

    // The import the app's add makes, with what only a terminal asks of it:
    // the count typed above, and the re-import.
    let mode = if reimport {
        ImportMode::Refresh
    } else {
        ImportMode::Fresh
    };
    let request = AddModelRequest {
        file_path: file_path.to_owned(),
    };
    let added = ops.add(request, param_count_override, mode).await?;

    // The components are linked as `gglib model update --component` links
    // them, through the rule that checks each file. The model is in the
    // library whatever that rule says, and a refusal says so.
    if components.is_some() {
        let request = UpdateModelRequest {
            components,
            ..UpdateModelRequest::default()
        };
        ops.update(added.id, request).await.with_context(|| {
            format!(
                "the model was added as id {}, but its components were not all linked",
                added.id
            )
        })?;
    }

    // `ops.add` answers with the row as a client lists it. The summary is of
    // the row as stored.
    let saved_model = ctx
        .app
        .models()
        .get_by_id(added.id)
        .await?
        .with_context(|| format!("model {} is no longer in the library", added.id))?;

    // Display clean summary using shared presentation
    if reimport {
        println!("\nRe-derived from file:");
    } else {
        println!("\nModel successfully created:");
    }
    display_model_summary(&saved_model, ModelSummaryOpts::with_title(""));

    if reimport {
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
#[path = "add_tests.rs"]
mod tests;
