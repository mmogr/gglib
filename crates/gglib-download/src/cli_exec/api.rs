//! `HuggingFace` API operations for CLI.
//!
//! Provides search, browse, and quantization listing for CLI commands.
//! These operations don't require database access.

use anyhow::{Result, anyhow};
use gglib_core::domain::Model;
use gglib_core::ports::GgufParserPort;
use gglib_core::ports::huggingface::{HfClientPort, read_roles, stored_label};
use gglib_hf::{DefaultHfClient, HfClientConfig};

use super::quant_listing::quant_listing;

/// A Hub client that sends `token`, when there is one, as a bearer token on
/// every request.
pub(super) fn hub_client(token: Option<String>) -> DefaultHfClient {
    DefaultHfClient::new(&HfClientConfig::default().with_optional_token(token))
}

/// List a repository's variants, each with what the head of its first file
/// says it is (read with `parser`), and its projectors apart.
pub async fn list_quantizations(
    model_id: &str,
    token: Option<String>,
    parser: &dyn GgufParserPort,
) -> Result<()> {
    println!("Finding available GGUF quantizations for {model_id}...");

    let client = hub_client(token);

    match HfClientPort::get_commit_sha(&client, model_id).await {
        Ok(sha) => {
            println!("Repository found: {model_id}");
            println!("Commit SHA: {sha}");
            println!("\nSearching for GGUF files using HuggingFace API...");

            let listed = tokio::try_join!(
                HfClientPort::list_quantizations(&client, model_id),
                HfClientPort::list_projectors(&client, model_id),
            );
            match listed {
                Ok((variants, projectors)) => {
                    let variants = read_roles(&client, parser, model_id, variants).await;
                    print!("{}", quant_listing(model_id, &variants, &projectors));
                }
                Err(e) => println!("Failed to fetch quantizations: {e}"),
            }
        }
        Err(e) => {
            println!("Failed to get repository info: {e}");
            println!("This might be due to a private repository or incorrect model ID");
        }
    }

    Ok(())
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
