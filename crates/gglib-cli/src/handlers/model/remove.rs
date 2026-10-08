//! Remove command handler.
//!
//! Removes a GGUF model from the database, through `ModelOps::remove`, the
//! operation the inspector's remove runs. The actual model file remains on
//! disk unchanged - only the database entry is removed.
//!
//! A model that is being served is refused, as the inspector's is, and the
//! refusal is worded here: it names what stops a server from a terminal,
//! which no flag of this command does.

use anyhow::{Result, anyhow};
use gglib_app_services::types::RemoveModelRequest;
use gglib_app_services::{GuiError, ModelOps};
use gglib_core::Model;

use super::resolver;
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
/// * `ctx` - The CLI context providing access to `AppCore`
/// * `ops` - The model operations the removal is made through
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
/// - The model is being served, as `ops` sees it, before the prompt or
///   after it
/// - User input fails
/// - Database removal operation fails
pub(crate) async fn execute(
    ctx: &CliContext,
    ops: &ModelOps,
    identifier: &str,
    force: bool,
) -> Result<()> {
    // First, try to find the model to show it to the user
    let model = resolver::resolve_model_identifier(ctx, identifier).await?;

    // Asked before the prompt, so nobody is asked to confirm a removal that
    // is then refused.
    let seen = ops.get(model.id).await?;
    if seen.is_serving {
        return Err(being_served(&model, seen.port));
    }

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

    // Removed by the id resolved above rather than handing the raw identifier
    // to a second lookup: the confirmation prompt sits between the two, so
    // they can disagree, and the second one reports in core's vocabulary.
    //
    // Never the request's `force`. That one stops the server a model is being
    // served from; `--force` here only skips the prompt above.
    ops.remove(model.id, RemoveModelRequest { force: false })
        .await
        .map_err(|refused| match refused {
            // The one conflict a removal has, met here when a server came up
            // while the prompt waited. `ops` words it for the inspector.
            GuiError::Conflict(_) => being_served(&model, None),
            other => other.into(),
        })?;
    let removed = &model;

    println!(
        "✓ Model '{}' (ID {}) successfully removed from database.",
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

/// What a terminal is told when the model it asked to remove is being
/// served, by the llama-server on `port` when that is known.
///
/// It names what stops a server from here. `ModelOps`' refusal says to stop
/// the server and not how, and no flag of this command does: `--force`
/// skips the prompt and stops nothing.
fn being_served(model: &Model, port: Option<u16>) -> anyhow::Error {
    let by = port.map_or_else(String::new, |port| {
        format!(" by a llama-server on port {port}")
    });
    anyhow!(
        "Model '{}' (ID {}) is being served{by}, so it was not removed.\n\
         Stop it first, in the gglib app or with `gglib daemon stop` (which stops \
         the daemon and every model it is serving), then remove it.",
        model.name,
        model.id
    )
}

#[cfg(test)]
#[path = "remove_tests.rs"]
mod tests;
