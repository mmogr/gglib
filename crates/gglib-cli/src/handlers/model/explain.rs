//! `gglib model explain` handler.
//!
//! Prints the launch gglib would make for a model, flag by flag with whose
//! choice each was, the context it would be served at and the fit behind
//! it, and its sampling parameters beside the layer that supplied each.
//!
//! Everything is [`ModelOps::explain`], the call `GET
//! /api/models/{id}/explain` answers with, so a terminal and the inspector
//! are told the same thing. Nothing here re-implements the launch, the fit
//! or the ladder, so this command cannot describe a launch that differs
//! from the one that runs.

use anyhow::{Result, anyhow};
use gglib_app_services::{GuiError, LaunchExplanationDto, ModelOps};

use super::resolver;
use crate::bootstrap::CliContext;
use crate::presentation::{explain_display, launch_display};

/// Execute `gglib model explain <id> [--profile NAME]`.
pub(crate) async fn execute(
    ctx: &CliContext,
    ops: &ModelOps,
    identifier: &str,
    profile: Option<&str>,
) -> Result<()> {
    let model = resolver::resolve_model_identifier(ctx, identifier).await?;
    let explanation = explain(ops, model.id, profile).await?;

    print!(
        "{}",
        launch_display::launch_text(&model.name, model.id, &explanation.plan)
    );
    explain_display::print_explanation(&model.name, model.id, &explanation.sampling);

    Ok(())
}

/// The explanation of model `id`: the launch it would make, and its stored
/// sampling with `profile` applied when one is named.
///
/// A name that is no configured profile is an error rather than a fall back
/// to no profile: someone who passed `--profile` wants to see that profile's
/// effect. Its message is the whole answer, so it is printed as it is.
async fn explain(ops: &ModelOps, id: i64, profile: Option<&str>) -> Result<LaunchExplanationDto> {
    let explained = ops.explain(id, profile).await;
    explained.map_err(|refused| match refused {
        GuiError::ValidationFailed(message) => anyhow!(message),
        other => other.into(),
    })
}

#[cfg(test)]
#[path = "explain_tests.rs"]
mod tests;
