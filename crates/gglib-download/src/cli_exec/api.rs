//! `HuggingFace` API operations for CLI.
//!
//! Provides search, browse, and quantization listing for CLI commands.
//! These operations don't require database access.

use anyhow::{Result, anyhow};
use gglib_core::domain::Model;
use gglib_core::ports::huggingface::stored_label;
use gglib_hf::{DefaultHfClient, HfClientConfig};

/// A Hub client that sends `token`, when there is one, as a bearer token on
/// every request.
pub(crate) fn hub_client(token: Option<String>) -> DefaultHfClient {
    DefaultHfClient::new(&HfClientConfig::default().with_optional_token(token))
}

/// The label a Hub lookup of `model` in `repo` asks for, as
/// [`stored_label`] reads it.
///
/// That is its stored variant label, else the label of the variant that
/// holds its file, asked of the Hub with `token`. `None` when neither names
/// one.
///
/// # Errors
///
/// When the repository cannot be listed.
pub async fn stored_variant_label(
    repo: &str,
    model: &Model,
    token: Option<String>,
) -> Result<Option<String>> {
    stored_label(&hub_client(token), repo, model)
        .await
        .map_err(|e| anyhow!("Failed to list {repo}'s variants: {e}"))
}
