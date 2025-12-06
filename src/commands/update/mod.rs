//! Update command implementation for modifying GGUF model metadata.
//!
//! This module provides functionality to update existing GGUF models in the database,
//! including model metadata, architecture information, and custom key-value pairs.

pub mod args;
pub mod metadata_ops;
mod preview;

#[cfg(test)]
mod tests;

use crate::services::AppCore;
use anyhow::Result;
use std::io::{self, Write};
use std::sync::Arc;
use tracing::warn;

// Re-export public API
pub use args::UpdateArgs;
pub use metadata_ops::{create_updated_model, parse_metadata_removals, parse_metadata_updates};

use metadata_ops::{
    parse_metadata_removals as parse_removals, parse_metadata_updates as parse_updates,
};
use preview::show_changes_preview;

/// Handle the update command from the CLI
pub async fn handle_update(core: Arc<AppCore>, args: UpdateArgs) -> Result<()> {
    execute(&core, args).await
}

/// Execute the update command
///
/// This function performs the core update logic, including model validation,
/// file checking, metadata processing, and database updates.
///
/// # Arguments
///
/// * `core` - A reference to the AppCore service layer
/// * `args` - The update command arguments
///
/// # Returns
///
/// Returns `Ok(())` if the update succeeds, or an error if the operation fails.
///
/// # Examples
///
/// ```rust,no_run
/// use gglib::commands::update::{execute, UpdateArgs};
/// use gglib::services::{database, AppCore};
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     let pool = database::setup_database().await?;
///     let core = AppCore::new(pool);
///     
///     let update_args = UpdateArgs {
///         id: 1,
///         name: Some("Updated Model Name".to_string()),
///         param_count: Some(13.0),
///         architecture: Some("mistral".to_string()),
///         quantization: Some("Q8_0".to_string()),
///         context_length: Some(8192),
///         metadata: vec!["version=2.0".to_string(), "tag=production".to_string()],
///         remove_metadata: None,
///         replace_metadata: false,
///         dry_run: false,
///         force: true, // Skip confirmation prompts
///     };
///     
///     execute(&core, update_args).await?;
///     println!("Model updated successfully!");
///     
///     Ok(())
/// }
/// ```
pub async fn execute(core: &AppCore, args: UpdateArgs) -> Result<()> {
    // First, get the existing model (returns error if not found)
    let existing_model = core.models().get_by_id(args.id).await?;

    // Verify the file still exists
    if !existing_model.file_path.exists() && !args.force {
        warn!(
            file_path = %existing_model.file_path.display(),
            "Model file no longer exists"
        );
        if !args.dry_run {
            print!("Continue with metadata update anyway? [y/N]: ");
            io::stdout().flush()?;
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            if !input.trim().to_lowercase().starts_with('y') {
                println!("Update cancelled.");
                return Ok(());
            }
        }
    }

    // Parse metadata changes
    let metadata_updates = parse_updates(&args.metadata)?;
    let metadata_removals = parse_removals(&args.remove_metadata)?;

    // Create the updated model
    let updated_model = metadata_ops::create_updated_model(
        &existing_model,
        &args,
        &metadata_updates,
        &metadata_removals,
    )?;

    // Show preview of changes
    show_changes_preview(&existing_model, &updated_model);

    if args.dry_run {
        println!("\n🔍 Dry run mode - no changes applied");
        return Ok(());
    }

    // Confirm changes unless force flag is used
    if !args.force {
        print!("\nApply these changes? [y/N]: ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if !input.trim().to_lowercase().starts_with('y') {
            println!("Update cancelled.");
            return Ok(());
        }
    }

    // Apply the updates
    core.models().update(args.id, &updated_model).await?;

    println!("✅ Model updated successfully!");
    Ok(())
}
