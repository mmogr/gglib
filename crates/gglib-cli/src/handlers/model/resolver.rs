//! Model identifier resolver.
//!
//! The one entry-point for resolving a user-supplied identifier (either a
//! numeric ID or a model name) to a [`Model`] record. Every `gglib model`
//! subcommand that takes an identifier goes through it, so all of them fail
//! the same way: one message, on stderr, with a non-zero exit.
//!
//! [`Model`]: gglib_core::domain::Model

use anyhow::{Result, anyhow};
use gglib_core::domain::Model;

use crate::bootstrap::CliContext;

/// Resolve a user-supplied identifier to a [`Model`].
///
/// Accepts either a numeric model ID or a model name.  If no model matches,
/// returns an error with a helpful message rather than `Ok(None)`, ensuring
/// consistent non-zero exit codes across all callers.
pub(crate) async fn resolve_model_identifier(ctx: &CliContext, identifier: &str) -> Result<Model> {
    ctx.app.models().get(identifier).await?.ok_or_else(|| {
        anyhow!(
            "No model found matching: '{identifier}'\n\
                 Use 'gglib model list' to see available models."
        )
    })
}
