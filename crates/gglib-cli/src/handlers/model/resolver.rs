//! Model identifier resolver.
//!
//! The one entry-point for resolving a user-supplied identifier (either a
//! numeric ID or a model name) to a [`Model`] record in this machine's
//! catalogue. Every command that needs the model its identifier names goes
//! through it, so all of them fail the same way: one message, on stderr, with
//! a non-zero exit. A lookup that has something to do on a miss is not a
//! resolution and does not come here: `Target::local_model` answers `None`,
//! because a session on `--port` may name a model this catalogue does not
//! hold. Neither takes a catalogue it could not read for a miss.
//!
//! A bare identifier means this machine, and is never sent anywhere else.
//! When it misses here and this machine is paired, a command that
//! `--remote` also reaches says so, because the id may well have come from
//! `gglib model list --remote`.
//!
//! [`Model`]: gglib_core::domain::Model

use anyhow::{Result, anyhow};
use gglib_core::RemotePairing;
use gglib_core::domain::{Model, ModelAction, UNNAMED_PAIRED};

use crate::bootstrap::CliContext;

/// Resolve a user-supplied identifier to a [`Model`], for a command that
/// changes this machine's library.
///
/// Accepts either a numeric model ID or a model name.  If no model matches,
/// returns an error with a helpful message rather than `Ok(None)`, ensuring
/// consistent non-zero exit codes across all callers.
pub(crate) async fn resolve_model_identifier(ctx: &CliContext, identifier: &str) -> Result<Model> {
    resolve_for(ctx, identifier, ModelAction::Manage).await
}

/// [`resolve_model_identifier`] for a command that does `action`.
///
/// A miss while this machine is paired, for an action the paired machine
/// allows, names that machine and the flag that reaches it. A failure to
/// read the catalogue is reported as itself, never as a miss.
pub(crate) async fn resolve_for(
    ctx: &CliContext,
    identifier: &str,
    action: ModelAction,
) -> Result<Model> {
    if let Some(model) = ctx.app.models().get(identifier).await? {
        return Ok(model);
    }
    let paired = if action.on_paired() {
        ctx.app
            .settings()
            .get()
            .await
            .ok()
            .and_then(|s| s.remote_pairing)
    } else {
        None
    };
    Err(anyhow!("{}", missing(identifier, paired.as_ref())))
}

/// What a miss says: the generic sentence, or, while `paired` is the stored
/// pairing, where that machine's models are.
fn missing(identifier: &str, paired: Option<&RemotePairing>) -> String {
    paired.map_or_else(
        || {
            format!(
                "No model found matching: '{identifier}'\n\
                 Use 'gglib model list' to see available models."
            )
        },
        |pairing| {
            format!(
                "no model {identifier} here; {}'s models need --remote (gglib model list --remote)",
                pairing.name.as_deref().unwrap_or(UNNAMED_PAIRED)
            )
        },
    )
}

#[cfg(test)]
#[path = "resolver_tests.rs"]
mod tests;
