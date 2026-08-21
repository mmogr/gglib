//! `HuggingFace` API operations for CLI.
//!
//! Provides search, browse, and quantization listing for CLI commands.
//! These operations don't require database access.

use anyhow::{Result, anyhow};
use gglib_core::ports::huggingface::HfClientPort;
use gglib_core::{Quantization, repo_short_name, strip_gguf_suffix};
use gglib_hf::{DefaultHfClient, HfClientConfig};
use hf_hub::api::sync::Api;
use reqwest::header::CONTENT_LENGTH;
use std::path::Path;
use std::time::Duration;

/// Create `HuggingFace` Hub API client.
pub fn create_hf_api(token: Option<String>, models_dir: &Path) -> Result<Api> {
    let mut api_builder = hf_hub::api::sync::ApiBuilder::new();

    if let Some(token) = token {
        api_builder = api_builder.with_token(Some(token));
    }

    // Set cache directory to our models directory
    let cache_dir = models_dir.join(".cache");
    api_builder = api_builder.with_cache_dir(cache_dir);

    api_builder
        .build()
        .map_err(|e| anyhow!("Failed to create HF API client: {e}"))
}

/// List available GGUF quantizations for a model.
pub async fn list_quantizations(
    model_id: &str,
    models_dir: &Path,
    token: Option<String>,
) -> Result<()> {
    println!("Finding available GGUF quantizations for {model_id}...");

    let api = create_hf_api(token.clone(), models_dir)?;
    let hf_api_repo = api.repo(hf_hub::Repo::with_revision(
        model_id.to_string(),
        hf_hub::RepoType::Model,
        "main".to_string(),
    ));

    match hf_api_repo.info() {
        Ok(info) => {
            println!("Repository found: {model_id}");
            println!("Commit SHA: {}", info.sha);
            println!("\nSearching for GGUF files using HuggingFace API...");

            let client = DefaultHfClient::new(&HfClientConfig::default());

            match client.list_quantizations(model_id).await {
                Ok(quantizations) => {
                    if quantizations.is_empty() {
                        println!("✗ No GGUF files found in this repository.");
                    } else {
                        println!("✓ Found {} quantizations:", quantizations.len());
                        for quant in &quantizations {
                            let shard_info = if quant.shard_count > 1 {
                                format!(" ({} shards)", quant.shard_count)
                            } else {
                                String::new()
                            };
                            #[allow(clippy::cast_precision_loss)]
                            let size_mib = quant.total_size as f64 / 1_048_576.0;
                            println!("  {} ({:.1} MiB){}", quant.name, size_mib, shard_info);
                        }

                        println!("\nTo download a specific quantization, use:");
                        for quant in &quantizations {
                            println!("  gglib model download {} -q {}", model_id, quant.name);
                        }
                    }
                }
                Err(e) => {
                    println!("Failed to fetch quantizations: {e}");
                    if let Err(err) = fallback_file_search(&hf_api_repo, model_id, token).await {
                        println!("Fallback pattern search also failed: {err}");
                    }
                }
            }
        }
        Err(e) => {
            println!("Failed to get repository info: {e}");
            println!("This might be due to a private repository or incorrect model ID");
        }
    }

    Ok(())
}

/// Build the list of candidate GGUF filenames to probe when the primary
/// `HuggingFace` quantization listing fails: one repo-name-prefixed
/// candidate per canonical quantization pattern (the dominant
/// `<repo>-<QUANT>.gguf` naming convention), plus a small set of bare,
/// unprefixed filenames (both cases) for repos shipping a single,
/// generically-named GGUF file.
fn fallback_candidates(model_name_clean: &str) -> Vec<String> {
    let mut candidates: Vec<String> = Quantization::canonical_patterns()
        .map(|pattern| format!("{model_name_clean}-{pattern}.gguf"))
        .collect();
    candidates.extend([
        "q8_0.gguf".to_string(),
        "Q8_0.gguf".to_string(),
        "q4_k_m.gguf".to_string(),
        "Q4_K_M.gguf".to_string(),
        "f16.gguf".to_string(),
        "F16.gguf".to_string(),
    ]);
    candidates
}

/// Fallback method for when API listing fails.
///
/// Probes candidate filenames via a HEAD request rather than downloading
/// each one, since a repository can contain many GB of files and this path
/// only needs to confirm existence.
async fn fallback_file_search(
    repo: &hf_hub::api::sync::ApiRepo,
    model_id: &str,
    token: Option<String>,
) -> Result<()> {
    println!("\nFalling back to pattern matching...");
    let mut found_files = Vec::new();

    let model_name_clean = strip_gguf_suffix(repo_short_name(model_id));
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| anyhow!("Failed to build HTTP client: {e}"))?;

    for pattern in fallback_candidates(model_name_clean) {
        let mut request = client.head(repo.url(&pattern));
        if let Some(ref tok) = token {
            request = request.header("Authorization", format!("Bearer {tok}"));
        }

        if let Ok(response) = request.send().await {
            if response.status().is_success() {
                let size_info = response
                    .headers()
                    .get(CONTENT_LENGTH)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .map_or_else(
                        || "size unknown".to_string(),
                        |bytes| {
                            #[allow(clippy::cast_precision_loss)]
                            let mib = bytes as f64 / 1_048_576.0;
                            format!("{mib:.1} MiB")
                        },
                    );
                println!("  ✓ {pattern} ({size_info})");
                found_files.push(pattern);
            }
        }
    }

    if found_files.is_empty() {
        println!("✗ No GGUF files found with common patterns.");
        println!("Try downloading directly if you know the exact quantization.");
    } else {
        println!(
            "✓ Found {} GGUF files using fallback method",
            found_files.len()
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_candidates_covers_legacy_and_new_patterns() {
        let candidates = fallback_candidates("llama-3-8b");
        assert!(candidates.contains(&"llama-3-8b-Q8_0.gguf".to_string()));
        assert!(candidates.contains(&"llama-3-8b-Q4_K_M.gguf".to_string()));
        assert!(candidates.contains(&"llama-3-8b-F16.gguf".to_string()));
        assert!(candidates.contains(&"q4_k_m.gguf".to_string()));
        // Previously missing from the old 9-pattern hardcoded list.
        assert!(candidates.contains(&"llama-3-8b-IQ4_XS.gguf".to_string()));
        assert!(candidates.contains(&"llama-3-8b-Q6_K.gguf".to_string()));
    }
}
