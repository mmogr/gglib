//! `HuggingFace` API operations for CLI.
//!
//! Provides search, browse, and quantization listing for CLI commands.
//! These operations don't require database access.

use anyhow::{Result, anyhow};
use gglib_core::ports::huggingface::HfClientPort;
use gglib_hf::{DefaultHfClient, HfClientConfig};
use hf_hub::api::sync::Api;
use std::path::Path;

use super::utils::format_number;

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

    let api = create_hf_api(token, models_dir)?;
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
                        println!("❌ No GGUF files found in this repository.");
                    } else {
                        println!("✅ Found {} quantizations:", quantizations.len());
                        for quant in &quantizations {
                            let shard_info = if quant.shard_count > 1 {
                                format!(" ({} shards)", quant.shard_count)
                            } else {
                                String::new()
                            };
                            #[allow(clippy::cast_precision_loss)]
                            let size_mb = quant.total_size as f64 / 1_048_576.0;
                            println!("  {} ({:.1} MB){}", quant.name, size_mb, shard_info);
                        }

                        println!("\nTo download a specific quantization, use:");
                        for quant in &quantizations {
                            println!("  gglib download {} -q {}", model_id, quant.name);
                        }
                    }
                }
                Err(e) => {
                    println!("Failed to fetch quantizations: {e}");
                    fallback_file_search(&hf_api_repo, model_id);
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

/// Fallback method for when API listing fails.
fn fallback_file_search(repo: &hf_hub::api::sync::ApiRepo, model_id: &str) {
    println!("\nFalling back to pattern matching...");
    let mut found_files = Vec::new();

    let model_name = model_id.split('/').next_back().unwrap_or("model");
    let model_name_clean = model_name.strip_suffix("-GGUF").unwrap_or(model_name);

    let specific_patterns = vec![
        format!("{}-Q8_0.gguf", model_name_clean),
        format!("{}-Q4_K_M.gguf", model_name_clean),
        format!("{}-F16.gguf", model_name_clean),
        "q8_0.gguf".to_string(),
        "Q8_0.gguf".to_string(),
        "q4_k_m.gguf".to_string(),
        "Q4_K_M.gguf".to_string(),
        "f16.gguf".to_string(),
        "F16.gguf".to_string(),
    ];

    for pattern in specific_patterns {
        if let Ok(path) = repo.get(&pattern) {
            #[allow(clippy::cast_precision_loss)]
            let size_info = std::fs::metadata(&path).map_or_else(
                |_| "size unknown".to_string(),
                |metadata| format!("{:.1} MB", metadata.len() as f64 / 1_048_576.0),
            );
            println!("  ✓ {pattern} ({size_info})");
            found_files.push(pattern);
        }
    }

    if found_files.is_empty() {
        println!("❌ No GGUF files found with common patterns.");
        println!("Try downloading directly if you know the exact quantization.");
    } else {
        println!(
            "✅ Found {} GGUF files using fallback method",
            found_files.len()
        );
    }
}

/// Search `HuggingFace` Hub for models.
pub async fn search_models(query: String, limit: u32, sort: String, gguf_only: bool) -> Result<()> {
    println!("🔍 Searching HuggingFace Hub for: '{query}'...");

    let client = DefaultHfClient::new(&HfClientConfig::default());
    let filter_gguf = gguf_only;

    let options = gglib_core::ports::huggingface::HfSearchOptions {
        query: Some(query.clone()),
        limit: if filter_gguf { limit * 3 } else { limit },
        page: 0,
        sort_by: sort.clone(),
        sort_ascending: false,
        min_params_b: None,
        max_params_b: None,
    };

    let response = client
        .search(&options)
        .await
        .map_err(|e| anyhow!("Search failed: {e}"))?;

    let mut filtered_models = Vec::new();

    for model in &response.items {
        let model_id = &model.model_id;
        if filter_gguf {
            if let Ok(quantizations) = client.list_quantizations(model_id).await {
                let names: Vec<String> = quantizations.iter().map(|q| q.name.clone()).collect();
                if !names.is_empty() {
                    filtered_models.push((model, names));
                }
            }
        } else if model_id.to_lowercase().contains("gguf")
            || model_id.to_lowercase().contains("llama")
            || model_id.to_lowercase().contains("mistral")
            || model_id.to_lowercase().contains("qwen")
        {
            if let Ok(quantizations) = client.list_quantizations(model_id).await {
                let names: Vec<String> = quantizations.iter().map(|q| q.name.clone()).collect();
                filtered_models.push((model, names));
            } else {
                filtered_models.push((model, Vec::new()));
            }
        }

        if filtered_models.len() >= limit as usize {
            break;
        }
    }

    if filtered_models.is_empty() {
        if filter_gguf {
            println!("No GGUF models found for query: '{query}'");
            println!(
                "💡 Try using a more general search term like 'gguf', 'llama-gguf', or specific model names"
            );
        } else {
            println!("No models found for query: '{query}'");
        }
        return Ok(());
    }

    let model_type = if filter_gguf { "GGUF " } else { "" };
    println!("\n📋 Found {} {}models:", filtered_models.len(), model_type);
    println!("{}", "─".repeat(80));

    for (i, (model, quantizations)) in filtered_models.iter().enumerate() {
        println!(
            " {}. {} (↓{} ❤{})",
            i + 1,
            model.model_id,
            format_number(model.downloads),
            model.likes
        );

        if !quantizations.is_empty() {
            println!("    Quantizations: {}", quantizations.join(", "));
        }

        if let Some(ref desc) = model.description {
            if !desc.is_empty() {
                let short_desc = if desc.len() > 80 {
                    format!("{}...", &desc[..77])
                } else {
                    desc.clone()
                };
                println!("    {short_desc}");
            }
        }

        println!();
    }

    println!("💡 To download a model: gglib download <model_id>");
    println!("💡 To list quantizations: gglib download <model_id> --list-quants");

    Ok(())
}

/// Browse popular `HuggingFace` models by category.
pub async fn browse_models(category: String, limit: u32, size: Option<String>) -> Result<()> {
    let sort_param = match category.as_str() {
        "recent" => "created",
        "trending" => "trending",
        // "popular" and anything else defaults to downloads
        _ => "downloads",
    };

    println!("🌐 Browsing {category} GGUF models...");

    let client = DefaultHfClient::new(&HfClientConfig::default());

    let search_query = size.as_ref().map_or_else(
        || "gguf".to_string(),
        |model_size| format!("gguf {model_size}"),
    );

    let options = gglib_core::ports::huggingface::HfSearchOptions {
        query: Some(search_query),
        limit,
        page: 0,
        sort_by: sort_param.to_string(),
        sort_ascending: false,
        min_params_b: None,
        max_params_b: None,
    };

    let response = client
        .search(&options)
        .await
        .map_err(|e| anyhow!("Search failed: {e}"))?;

    if response.items.is_empty() {
        println!("No {category} models found.");
        return Ok(());
    }

    println!("\n🏆 {} GGUF Models:", category.to_uppercase());
    println!("{}", "─".repeat(80));

    for (i, model) in response.items.iter().enumerate() {
        println!(
            "{:2}. {} (↓{} ❤{})",
            i + 1,
            model.model_id,
            format_number(model.downloads),
            model.likes
        );

        if let Ok(quantizations) = client.list_quantizations(&model.model_id).await {
            let names: Vec<&str> = quantizations.iter().map(|q| q.name.as_str()).collect();
            if !names.is_empty() {
                println!("    Quantizations: {}", names.join(", "));
            }
        }

        if let Some(ref desc) = model.description {
            if !desc.is_empty() {
                let short_desc = if desc.len() > 100 {
                    format!("{}...", &desc[..97])
                } else {
                    desc.clone()
                };
                println!("    {short_desc}");
            }
        }

        println!();
    }

    println!("💡 To download a model: gglib download <model_id>");
    println!("💡 To see all quantizations: gglib download <model_id> --list-quants");

    Ok(())
}
