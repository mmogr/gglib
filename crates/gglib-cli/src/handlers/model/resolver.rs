//! Model identifier resolver.
//!
//! Provides a single entry-point for resolving a user-supplied identifier
//! (either a numeric ID or a model name) to a [`Model`] record.  All model
//! command handlers use this instead of calling `get_by_id` directly, giving
//! every command consistent name-or-id resolution and error messaging.
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
pub async fn resolve_model_identifier(ctx: &CliContext, identifier: &str) -> Result<Model> {
    ctx.app
        .models()
        .get(identifier)
        .await?
        .ok_or_else(|| {
            anyhow!(
                "No model found matching: '{identifier}'\n\
                 Use 'gglib model list' to see available models."
            )
        })
}
