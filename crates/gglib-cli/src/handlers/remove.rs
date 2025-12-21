//! Remove command handler.
//!
//! Removes a GGUF model from the database. The actual model file
//! remains on disk unchanged - only the database entry is removed.

use anyhow::Result;

use crate::bootstrap::CliContext;
use crate::presentation::{ModelSummaryOpts, display_model_summary};
use crate::utils::input;

/// Execute the remove command.
///
/// Searches for a model matching the provided identifier (ID or name),
/// confirms the deletion with the user (unless force flag is used),
/// and removes the model entry from the database.
///
/// # Arguments
///
/// * `ctx` - The CLI context providing access to AppCore
/// * `identifier` - The name or ID of the model to remove
/// * `force` - If true, skips confirmation prompt
///
/// # Returns
///
/// Returns `Result<()>` indicating success or failure.
///
/// # Errors
///
/// This function will return an error if:
/// - Model not found
/// - User input fails
/// - Database removal operation fails
pub async fn execute(ctx: &CliContext, identifier: &str, force: bool) -> Result<()> {
    // First, try to find the model to show it to the user
    let model = match ctx.app().models().get(identifier).await? {
        Some(m) => m,
        None => {
            println!("No model found matching: '{identifier}'");
            println!("Use 'gglib list' to see available models.");
            return Ok(());
        }
    };

    if !force {
        display_model_summary(&model, ModelSummaryOpts::for_removal());
        println!();

        let confirm = input::prompt_confirmation(
            "Are you sure you want to remove this model from the database?",
        )?;
        if !confirm {
            println!("Remove operation cancelled.");
            return Ok(());
        }
    }

    // Remove the model
    let removed = ctx.app().models().remove(identifier).await?;

    println!(
        "✅ Model '{}' (ID {}) successfully removed from database.",
        removed.name, removed.id
    );

    if !force {
        println!(
            "Note: The model file '{}' remains on disk.",
            removed.file_path.display()
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_identifier_type_detection() {
        fn is_numeric_id(identifier: &str) -> bool {
            identifier.parse::<i64>().is_ok()
        }

        assert!(is_numeric_id("123"));
        assert!(is_numeric_id("0"));
        assert!(!is_numeric_id("model_name"));
        assert!(!is_numeric_id("123abc"));
        assert!(!is_numeric_id(""));
    }
}
