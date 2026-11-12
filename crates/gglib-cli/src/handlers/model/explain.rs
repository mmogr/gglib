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
//!
//! While the daemon that serves this library runs, the explanation is that
//! route's answer: the daemon plans over the models it has loaded, and so
//! says which one a Load would unload first. Without it nothing is loaded,
//! and the command's own `ModelOps` plans over nothing.

use anyhow::{Result, anyhow};
use gglib_app_services::{GuiError, LaunchExplanationDto, ModelOps};

use super::resolver;
use crate::bootstrap::CliContext;
use crate::daemon_client::{auth, running};
use crate::presentation::{explain_display, launch_display};

/// Execute `gglib model explain <id> [--profile NAME]`.
pub(crate) async fn execute(
    ctx: &CliContext,
    ops: &ModelOps,
    identifier: &str,
    profile: Option<&str>,
) -> Result<()> {
    let model = resolver::resolve_model_identifier(ctx, identifier).await?;
    let explanation = explanation(ops, model.id, profile).await?;

    print!(
        "{}",
        launch_display::launch_text(&model.name, model.id, &explanation.plan)
    );
    explain_display::print_explanation(&model.name, model.id, &explanation.sampling);

    Ok(())
}

/// The explanation of model `id`, from the daemon that serves this library
/// when it is running and from `ops` when it is not.
///
/// That daemon is found as `LibraryChanges::tell_daemon` finds it: by the
/// token it left under this data root, which is the credential presented.
/// A token left by a daemon that has stopped, or refused by one on the port
/// that serves another library, finds none. Once it is there, its answer or
/// its refusal is the answer; a plan made here would not know what it has
/// loaded.
async fn explanation(
    ops: &ModelOps,
    id: i64,
    profile: Option<&str>,
) -> Result<LaunchExplanationDto> {
    if auth::daemon_token().is_some()
        && let Ok(daemon) = running().await
        && let Some(explained) = daemon.explain_model(id, profile).await?
    {
        return Ok(explained);
    }
    explain(ops, id, profile).await
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

#[cfg(test)]
#[path = "explain_daemon_tests.rs"]
mod daemon_tests;
