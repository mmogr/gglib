#![allow(clippy::collapsible_if)]

use anyhow::{Result, anyhow};
use hf_hub::api::sync::Api;
use std::path::Path;

use super::utils::format_number;
use crate::services::core::HuggingFaceService;

/// Create HuggingFace Hub API client
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
        .map_err(|e| anyhow!("Failed to create HF API client: {}", e))
}

/// List available GGUF quantizations for a model using HF API
pub async fn list_available_quantizations(api: &Api, model_id: &str) -> Result<()> {
    println!("Finding available GGUF quantizations for {}...", model_id);

    let repo = api.repo(hf_hub::Repo::with_revision(
        model_id.to_string(),
        hf_hub::RepoType::Model,
        "main".to_string(),
    ));

    match repo.info() {
        Ok(info) => {
            println!("Repository found: {}", model_id);
            println!("Commit SHA: {}", info.sha);

            println!("\nSearching for GGUF files using HuggingFace API...");

            // Use HuggingFaceService for consistent API access (DRY)
            let hf_service = HuggingFaceService::new();

            match hf_service.get_quantizations(model_id).await {
                Ok(response) => {
                    if response.quantizations.is_empty() {
                        println!("❌ No GGUF files found in this repository.");
                    } else {
                        println!("✅ Found {} quantizations:", response.quantizations.len());
                        for quant in &response.quantizations {
                            let shard_info = if quant.is_sharded {
                                format!(" ({} shards)", quant.shard_count.unwrap_or(0))
                            } else {
                                String::new()
                            };
                            println!("  {} ({:.1} MB){}", quant.name, quant.size_mb, shard_info);
                        }

                        println!("\nTo download a specific quantization, use:");
                        for quant in &response.quantizations {
                            println!("  gglib download {} -q {}", model_id, quant.name);
                        }
                    }
                }
                Err(e) => {
                    println!("Failed to fetch quantizations: {}", e);
                    fallback_file_search(&repo, model_id).await?;
                }
            }
        }
        Err(e) => {
            println!("Failed to get repository info: {}", e);
            println!("This might be due to a private repository or incorrect model ID");
        }
    }

    Ok(())
}

/// Fallback method for when API listing fails
async fn fallback_file_search(repo: &hf_hub::api::sync::ApiRepo, model_id: &str) -> Result<()> {
    println!("\nFalling back to pattern matching...");
    let mut found_files = Vec::new();

    // Since we can't easily glob, try some educated guesses based on model name
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
            let size_info = match std::fs::metadata(&path) {
                Ok(metadata) => format!("{:.1} MB", metadata.len() as f64 / 1_048_576.0),
                Err(_) => "size unknown".to_string(),
            };
            println!("  ✓ {} ({})", pattern, size_info);
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

    Ok(())
}

/// Handle search command for HuggingFace Hub
pub async fn handle_search(query: String, limit: u32, sort: String, gguf_only: bool) -> Result<()> {
    println!("🔍 Searching HuggingFace Hub for: '{}'...", query);

    let hf_service = HuggingFaceService::new();

    // Default to GGUF filtering unless explicitly disabled
    let filter_gguf = gguf_only;

    // Use the service to fetch models
    let models_array = hf_service
        .search_models(&query, if filter_gguf { limit * 3 } else { limit }, &sort)
        .await?;

    let mut filtered_models = Vec::new();

    // Filter models based on gguf_only flag
    for model in &models_array {
        if let Some(model_id) = model.get("id").and_then(|v| v.as_str()) {
            if filter_gguf {
                // Check if model actually has GGUF files
                if let Ok(quantizations) = hf_service.get_quantization_names(model_id).await {
                    if !quantizations.is_empty() {
                        filtered_models.push((model, quantizations));
                    }
                }
            } else {
                // Just check for any potential GGUF indicators in the name
                if model_id.to_lowercase().contains("gguf")
                    || model_id.to_lowercase().contains("llama")
                    || model_id.to_lowercase().contains("mistral")
                    || model_id.to_lowercase().contains("qwen")
                {
                    // Quick check for GGUF files
                    if let Ok(quantizations) = hf_service.get_quantization_names(model_id).await {
                        filtered_models.push((model, quantizations));
                    } else {
                        filtered_models.push((model, Vec::new()));
                    }
                }
            }

            // Limit results
            if filtered_models.len() >= limit as usize {
                break;
            }
        }
    }

    if filtered_models.is_empty() {
        if filter_gguf {
            println!("No GGUF models found for query: '{}'", query);
            println!(
                "💡 Try using a more general search term like 'gguf', 'llama-gguf', or specific model names"
            );
        } else {
            println!("No models found for query: '{}'", query);
        }
        return Ok(());
    }

    let model_type = if filter_gguf { "GGUF " } else { "" };
    println!("\n📋 Found {} {}models:", filtered_models.len(), model_type);
    println!("{}", "─".repeat(80));

    for (i, (model, quantizations)) in filtered_models.iter().enumerate() {
        if let Some(model_id) = model.get("id").and_then(|v| v.as_str()) {
            let downloads = model.get("downloads").and_then(|v| v.as_u64()).unwrap_or(0);

            let likes = model.get("likes").and_then(|v| v.as_u64()).unwrap_or(0);

            println!(
                " {}. {} (↓{} ❤{})",
                i + 1,
                model_id,
                format_number(downloads),
                likes
            );

            // Show available quantizations for GGUF models
            if !quantizations.is_empty() {
                println!("    Quantizations: {}", quantizations.join(", "));
            }

            if let Some(desc) = model.get("description").and_then(|v| v.as_str()) {
                if !desc.is_empty() {
                    let short_desc = if desc.len() > 80 {
                        format!("{}...", &desc[..77])
                    } else {
                        desc.to_string()
                    };
                    println!("    {}", short_desc);
                }
            }

            println!();
        }
    }

    println!("💡 To download a model: gglib download <model_id>");
    println!("💡 To list quantizations: gglib download <model_id> --list-quants");

    Ok(())
}

/// Handle browse command for popular HuggingFace models
pub async fn handle_browse(category: String, limit: u32, size: Option<String>) -> Result<()> {
    let sort_param = match category.as_str() {
        "popular" => "downloads",
        "recent" => "created",
        "trending" => "trending",
        _ => "downloads",
    };

    println!("🌐 Browsing {} GGUF models...", category);

    let hf_service = HuggingFaceService::new();

    // Search for models with GGUF-related tags
    let search_query = if let Some(ref model_size) = size {
        format!("gguf {}", model_size)
    } else {
        "gguf".to_string()
    };

    // Use the service to fetch models
    let models_array = hf_service
        .search_models(&search_query, limit, sort_param)
        .await?;

    if models_array.is_empty() {
        println!("No {} models found.", category);
        return Ok(());
    }

    println!("\n🏆 {} GGUF Models:", category.to_uppercase());
    println!("{}", "─".repeat(80));

    for (i, model) in models_array.iter().enumerate() {
        if let Some(model_id) = model.get("id").and_then(|v| v.as_str()) {
            let downloads = model.get("downloads").and_then(|v| v.as_u64()).unwrap_or(0);

            let likes = model.get("likes").and_then(|v| v.as_u64()).unwrap_or(0);

            println!(
                "{:2}. {} (↓{} ❤{})",
                i + 1,
                model_id,
                format_number(downloads),
                likes
            );

            // Show available quantizations
            if let Ok(quantizations) = hf_service.get_quantization_names(model_id).await {
                if !quantizations.is_empty() {
                    println!("    Quantizations: {}", quantizations.join(", "));
                }
            }

            if let Some(desc) = model.get("description").and_then(|v| v.as_str()) {
                if !desc.is_empty() {
                    let short_desc = if desc.len() > 100 {
                        format!("{}...", &desc[..97])
                    } else {
                        desc.to_string()
                    };
                    println!("    {}", short_desc);
                }
            }

            println!();
        }
    }

    println!("💡 To download a model: gglib download <model_id>");
    println!("💡 To see all quantizations: gglib download <model_id> --list-quants");

    Ok(())
}
