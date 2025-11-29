#![allow(clippy::collapsible_if)]

use anyhow::{Result, anyhow};
use hf_hub::api::sync::Api;
use reqwest;
use serde_json;
use std::path::Path;

use super::file_ops::extract_quantization_from_filename;
use super::utils::format_number;
use crate::models::gui::{
    HfModelSummary, HfQuantization, HfQuantizationsResponse, HfSearchRequest, HfSearchResponse,
};

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

            // Use HuggingFace's REST API to list files (including sharded directories)
            let api_url = format!("https://huggingface.co/api/models/{}/tree/main", model_id);

            match reqwest::get(&api_url).await {
                Ok(response) => {
                    if response.status().is_success() {
                        match response.text().await {
                            Ok(json_text) => {
                                match serde_json::from_str::<serde_json::Value>(&json_text) {
                                    Ok(data) => {
                                        let mut gguf_files = Vec::new();

                                        if let Some(files) = data.as_array() {
                                            // 1) Direct GGUF files at repo root
                                            for file in files {
                                                if let (Some(filename), Some(size)) = (
                                                    file.get("path").and_then(|v| v.as_str()),
                                                    file.get("size").and_then(|v| v.as_u64()),
                                                ) {
                                                    let entry_type = file
                                                        .get("type")
                                                        .and_then(|v| v.as_str())
                                                        .unwrap_or("file");

                                                    if entry_type == "file"
                                                        && filename.ends_with(".gguf")
                                                    {
                                                        let size_mb = size as f64 / 1_048_576.0;
                                                        gguf_files
                                                            .push((filename.to_string(), size_mb));
                                                    }
                                                }
                                            }

                                            // 2) Sharded GGUF files in per-quant directories
                                            for file in files {
                                                if let Some(dir_path) =
                                                    file.get("path").and_then(|v| v.as_str())
                                                {
                                                    let entry_type = file
                                                        .get("type")
                                                        .and_then(|v| v.as_str())
                                                        .unwrap_or("file");

                                                    if entry_type == "directory" {
                                                        let sub_api_url = format!(
                                                            "https://huggingface.co/api/models/{}/tree/main/{}",
                                                            model_id, dir_path
                                                        );

                                                        if let Ok(sub_response) =
                                                            reqwest::get(&sub_api_url).await
                                                        {
                                                            if sub_response.status().is_success() {
                                                                if let Ok(sub_json_text) =
                                                                    sub_response.text().await
                                                                {
                                                                    if let Ok(sub_data) =
                                                                        serde_json::from_str::<
                                                                            serde_json::Value,
                                                                        >(
                                                                            &sub_json_text
                                                                        )
                                                                    {
                                                                        if let Some(sub_files) =
                                                                            sub_data.as_array()
                                                                        {
                                                                            for sub_file in
                                                                                sub_files
                                                                            {
                                                                                if let (
                                                                                    Some(sub_path),
                                                                                    Some(sub_size),
                                                                                ) = (
                                                                                    sub_file
                                                                                        .get("path")
                                                                                        .and_then(
                                                                                            |v| {
                                                                                                v
                                                                                        .as_str()
                                                                                            },
                                                                                        ),
                                                                                    sub_file
                                                                                        .get("size")
                                                                                        .and_then(
                                                                                            |v| {
                                                                                                v
                                                                                        .as_u64()
                                                                                            },
                                                                                        ),
                                                                                ) {
                                                                                    if sub_path
                                                                                        .ends_with(
                                                                                            ".gguf",
                                                                                        )
                                                                                    {
                                                                                        let size_mb = sub_size as f64
                                                                                            / 1_048_576.0;
                                                                                        gguf_files.push((
                                                                                            sub_path
                                                                                                .to_string(),
                                                                                            size_mb,
                                                                                        ));
                                                                                    }
                                                                                }
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }

                                        if gguf_files.is_empty() {
                                            println!("❌ No GGUF files found in this repository.");
                                        } else {
                                            println!("✅ Found {} GGUF files:", gguf_files.len());
                                            for (filename, size_mb) in &gguf_files {
                                                let quant =
                                                    extract_quantization_from_filename(filename);
                                                println!(
                                                    "  {} ({:.1} MB) - quantization: {}",
                                                    filename, size_mb, quant
                                                );
                                            }

                                            println!("\nTo download a specific file, use:");
                                            for (filename, _) in &gguf_files {
                                                let quant =
                                                    extract_quantization_from_filename(filename);
                                                if quant != "unknown" {
                                                    println!(
                                                        "  gglib download {} -q {}",
                                                        model_id, quant
                                                    );
                                                }
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        println!("Failed to parse API response: {}", e);
                                        fallback_file_search(&repo, model_id).await?;
                                    }
                                }
                            }
                            Err(e) => {
                                println!("Failed to read API response: {}", e);
                                fallback_file_search(&repo, model_id).await?;
                            }
                        }
                    } else {
                        println!("API request failed with status: {}", response.status());
                        fallback_file_search(&repo, model_id).await?;
                    }
                }
                Err(e) => {
                    println!("Failed to make API request: {}", e);
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

/// Get available quantizations for a model
async fn get_model_quantizations(model_id: &str) -> Result<Vec<String>> {
    let api_url = format!("https://huggingface.co/api/models/{}/tree/main", model_id);
    let mut quantizations = Vec::new();

    if let Ok(response) = reqwest::get(&api_url).await {
        if response.status().is_success() {
            if let Ok(data) = response.json::<serde_json::Value>().await {
                if let Some(files) = data.as_array() {
                    // 1) Direct GGUF files at repo root
                    for file in files {
                        if let Some(filename) = file.get("path").and_then(|v| v.as_str()) {
                            let entry_type =
                                file.get("type").and_then(|v| v.as_str()).unwrap_or("file");

                            if entry_type == "file" && filename.ends_with(".gguf") {
                                let quant = extract_quantization_from_filename(filename);
                                if quant != "unknown" && !quantizations.contains(&quant.to_string())
                                {
                                    quantizations.push(quant.to_string());
                                }
                            }
                        }
                    }

                    // 2) Sharded GGUF files in per-quant directories
                    for file in files {
                        if let Some(dir_path) = file.get("path").and_then(|v| v.as_str()) {
                            let entry_type =
                                file.get("type").and_then(|v| v.as_str()).unwrap_or("file");

                            if entry_type == "directory" {
                                let sub_api_url = format!(
                                    "https://huggingface.co/api/models/{}/tree/main/{}",
                                    model_id, dir_path
                                );

                                if let Ok(sub_response) = reqwest::get(&sub_api_url).await {
                                    if sub_response.status().is_success() {
                                        if let Ok(sub_data) =
                                            sub_response.json::<serde_json::Value>().await
                                        {
                                            if let Some(sub_files) = sub_data.as_array() {
                                                for sub_file in sub_files {
                                                    if let Some(sub_path) = sub_file
                                                        .get("path")
                                                        .and_then(|v| v.as_str())
                                                    {
                                                        if sub_path.ends_with(".gguf") {
                                                            let quant =
                                                                extract_quantization_from_filename(
                                                                    sub_path,
                                                                );
                                                            if quant != "unknown"
                                                                && !quantizations
                                                                    .contains(&quant.to_string())
                                                            {
                                                                quantizations
                                                                    .push(quant.to_string());
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    quantizations.sort();
    Ok(quantizations)
}

/// Handle search command for HuggingFace Hub
pub async fn handle_search(query: String, limit: u32, sort: String, gguf_only: bool) -> Result<()> {
    println!("🔍 Searching HuggingFace Hub for: '{}'...", query);

    // Default to GGUF filtering unless explicitly disabled
    let filter_gguf = gguf_only;

    let search_url = format!(
        "https://huggingface.co/api/models?search={}&limit={}&sort={}&direction=-1",
        urlencoding::encode(&query),
        if filter_gguf { limit * 3 } else { limit }, // Get more results when filtering
        sort
    );

    let response = reqwest::get(&search_url).await?;

    if !response.status().is_success() {
        return Err(anyhow!("Search request failed: {}", response.status()));
    }

    let models: serde_json::Value = response.json().await?;

    if let Some(models_array) = models.as_array() {
        let mut filtered_models = Vec::new();

        // Filter models based on gguf_only flag
        for model in models_array {
            if let Some(model_id) = model.get("id").and_then(|v| v.as_str()) {
                if filter_gguf {
                    // Check if model actually has GGUF files
                    if let Ok(quantizations) = get_model_quantizations(model_id).await {
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
                        if let Ok(quantizations) = get_model_quantizations(model_id).await {
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
    }

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

    // Search for models with GGUF-related tags
    let search_query = if let Some(ref model_size) = size {
        format!("gguf {}", model_size)
    } else {
        "gguf".to_string()
    };

    let browse_url = format!(
        "https://huggingface.co/api/models?search={}&limit={}&sort={}&library=gguf&direction=-1",
        urlencoding::encode(&search_query),
        limit,
        sort_param
    );

    let response = reqwest::get(&browse_url).await?;

    if !response.status().is_success() {
        return Err(anyhow!("Browse request failed: {}", response.status()));
    }

    let models: serde_json::Value = response.json().await?;

    if let Some(models_array) = models.as_array() {
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
                if let Ok(quantizations) = get_model_quantizations(model_id).await {
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
    }

    Ok(())
}

// ============================================================================
// GUI Browser API Functions
// ============================================================================

/// Search HuggingFace models for the GUI browser with pagination and parameter filtering.
///
/// This function queries the HuggingFace API for GGUF text-generation models,
/// applies client-side parameter filtering, and returns paginated results.
pub async fn search_hf_models_paginated(request: HfSearchRequest) -> Result<HfSearchResponse> {
    // Fetch more models than requested since we filter out models without actual GGUF files.
    // The library=gguf filter returns models TAGGED with GGUF, but many are base models
    // that don't contain GGUF files themselves (only their derivatives do).
    // Fetching 100 and filtering typically yields ~15-20 models with actual GGUF files.
    let fetch_limit = 100;

    // Use expand[]=siblings&expand[]=safetensors to get file list AND parameter count
    // We need siblings to filter for models that actually contain .gguf files
    let mut url = format!(
        "https://huggingface.co/api/models?library=gguf&pipeline_tag=text-generation&expand[]=siblings&expand[]=safetensors&sort=downloads&direction=-1&limit={}&p={}",
        fetch_limit, request.page
    );

    // CRITICAL: Always add "GGUF" to the search to filter for repos that actually contain
    // GGUF files. The library=gguf tag returns base models like "meta-llama/Llama-3.1-8B"
    // that are tagged with GGUF because derivatives exist, but don't contain GGUF files.
    // Adding "GGUF" to the search returns repos like "bartowski/Llama-3.1-8B-GGUF" that
    // actually contain the quantized files.
    let search_query = match &request.query {
        Some(q) if !q.trim().is_empty() => {
            // If user query doesn't already contain "gguf", append it
            if q.to_lowercase().contains("gguf") {
                q.trim().to_string()
            } else {
                format!("{} GGUF", q.trim())
            }
        }
        _ => "GGUF".to_string(),
    };
    url.push_str(&format!("&search={}", urlencoding::encode(&search_query)));

    let response = reqwest::get(&url).await?;

    if !response.status().is_success() {
        return Err(anyhow!(
            "HuggingFace API request failed: {}",
            response.status()
        ));
    }

    // Check for pagination via Link header
    let has_more = response
        .headers()
        .get("Link")
        .and_then(|h| h.to_str().ok())
        .map(|link| link.contains("rel=\"next\""))
        .unwrap_or(false);

    let models_json: Vec<serde_json::Value> = response.json().await?;

    // Parse and filter models
    let mut models: Vec<HfModelSummary> = Vec::new();

    for model_json in models_json {
        // Check if the model actually contains .gguf files
        // The library=gguf filter returns models tagged with GGUF, but some are base models
        // that don't contain GGUF files themselves (only derivatives do)
        let has_gguf_files = model_json
            .get("siblings")
            .and_then(|s| s.as_array())
            .map(|siblings| {
                siblings.iter().any(|file| {
                    file.get("rfilename")
                        .and_then(|f| f.as_str())
                        .map(|name| name.ends_with(".gguf"))
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false);

        // Skip models that don't actually have GGUF files
        if !has_gguf_files {
            continue;
        }

        // Extract parameter count from safetensors.total
        let parameters_b = model_json
            .get("safetensors")
            .and_then(|s| s.get("total"))
            .and_then(|t| t.as_u64())
            .map(|params| params as f64 / 1_000_000_000.0);

        // Apply parameter filtering (client-side)
        if let Some(min) = request.min_params_b {
            if let Some(params) = parameters_b {
                if params < min {
                    continue;
                }
            } else {
                // Skip models without parameter info when filtering by min
                continue;
            }
        }

        if let Some(max) = request.max_params_b {
            if let Some(params) = parameters_b {
                if params > max {
                    continue;
                }
            } else {
                // Skip models without parameter info when filtering by max
                continue;
            }
        }

        let id = model_json
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        if id.is_empty() {
            continue;
        }

        // Extract author from id (format: "author/model-name")
        let author = id.split('/').next().map(|s| s.to_string());

        // Extract model name (last part of id)
        let name = id.split('/').next_back().unwrap_or(&id).to_string();

        let downloads = model_json
            .get("downloads")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let likes = model_json
            .get("likes")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let last_modified = model_json
            .get("lastModified")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let description = model_json
            .get("description")
            .and_then(|v| v.as_str())
            .map(|s| {
                // Truncate long descriptions
                if s.len() > 200 {
                    format!("{}...", &s[..197])
                } else {
                    s.to_string()
                }
            });

        let tags = model_json
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| t.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        models.push(HfModelSummary {
            id,
            name,
            author,
            downloads,
            likes,
            last_modified,
            parameters_b,
            description,
            tags,
        });
    }

    Ok(HfSearchResponse {
        models,
        has_more,
        page: request.page,
        total_count: None, // HuggingFace API doesn't provide total count
    })
}

/// Get available quantizations for a model with structured data for the GUI.
///
/// Returns detailed information about each quantization variant including
/// file size and whether it's sharded.
pub async fn get_quantizations_structured(model_id: &str) -> Result<HfQuantizationsResponse> {
    let api_url = format!("https://huggingface.co/api/models/{}/tree/main", model_id);
    let mut quantizations: Vec<HfQuantization> = Vec::new();

    let response = reqwest::get(&api_url).await?;

    if !response.status().is_success() {
        return Err(anyhow!(
            "Failed to fetch model files: {}",
            response.status()
        ));
    }

    let data: serde_json::Value = response.json().await?;

    if let Some(files) = data.as_array() {
        // Track quantizations we've seen (to handle sharded models)
        let mut quant_map: std::collections::HashMap<String, HfQuantization> =
            std::collections::HashMap::new();

        // 1) Direct GGUF files at repo root
        for file in files {
            if let (Some(path), Some(size)) = (
                file.get("path").and_then(|v| v.as_str()),
                file.get("size").and_then(|v| v.as_u64()),
            ) {
                let entry_type = file.get("type").and_then(|v| v.as_str()).unwrap_or("file");

                if entry_type == "file" && path.ends_with(".gguf") {
                    let quant_name = extract_quantization_from_filename(path).to_string();
                    if quant_name != "unknown" {
                        let size_mb = size as f64 / 1_048_576.0;

                        // Check if this is a shard (e.g., model-00001-of-00003.gguf)
                        let is_shard = path.contains("-00001-of-") || path.contains("-00002-of-");

                        if let Some(existing) = quant_map.get_mut(&quant_name) {
                            // Add to existing shard count
                            existing.size_bytes += size;
                            existing.size_mb += size_mb;
                            if let Some(ref mut count) = existing.shard_count {
                                *count += 1;
                            }
                        } else {
                            quant_map.insert(
                                quant_name.clone(),
                                HfQuantization {
                                    name: quant_name,
                                    file_path: path.to_string(),
                                    size_bytes: size,
                                    size_mb,
                                    is_sharded: is_shard,
                                    shard_count: if is_shard { Some(1) } else { None },
                                },
                            );
                        }
                    }
                }
            }
        }

        // 2) Check subdirectories for sharded GGUF files
        for file in files {
            if let Some(dir_path) = file.get("path").and_then(|v| v.as_str()) {
                let entry_type = file.get("type").and_then(|v| v.as_str()).unwrap_or("file");

                if entry_type == "directory" {
                    let sub_api_url = format!(
                        "https://huggingface.co/api/models/{}/tree/main/{}",
                        model_id, dir_path
                    );

                    if let Ok(sub_response) = reqwest::get(&sub_api_url).await {
                        if sub_response.status().is_success() {
                            if let Ok(sub_data) = sub_response.json::<serde_json::Value>().await {
                                if let Some(sub_files) = sub_data.as_array() {
                                    let mut dir_total_size: u64 = 0;
                                    let mut dir_shard_count: u32 = 0;
                                    let mut dir_quant_name: Option<String> = None;
                                    let mut dir_first_file: Option<String> = None;

                                    for sub_file in sub_files {
                                        if let (Some(sub_path), Some(sub_size)) = (
                                            sub_file.get("path").and_then(|v| v.as_str()),
                                            sub_file.get("size").and_then(|v| v.as_u64()),
                                        ) {
                                            if sub_path.ends_with(".gguf") {
                                                dir_total_size += sub_size;
                                                dir_shard_count += 1;

                                                if dir_quant_name.is_none() {
                                                    dir_quant_name = Some(
                                                        extract_quantization_from_filename(
                                                            sub_path,
                                                        )
                                                        .to_string(),
                                                    );
                                                    dir_first_file = Some(sub_path.to_string());
                                                }
                                            }
                                        }
                                    }

                                    // Add this directory's quantization if we found GGUF files
                                    if let (Some(quant_name), Some(first_file)) =
                                        (dir_quant_name, dir_first_file)
                                    {
                                        if quant_name != "unknown"
                                            && !quant_map.contains_key(&quant_name)
                                        {
                                            quant_map.insert(
                                                quant_name.clone(),
                                                HfQuantization {
                                                    name: quant_name,
                                                    file_path: first_file,
                                                    size_bytes: dir_total_size,
                                                    size_mb: dir_total_size as f64 / 1_048_576.0,
                                                    is_sharded: dir_shard_count > 1,
                                                    shard_count: if dir_shard_count > 1 {
                                                        Some(dir_shard_count)
                                                    } else {
                                                        None
                                                    },
                                                },
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Convert map to vec and sort by quantization name
        quantizations = quant_map.into_values().collect();
        quantizations.sort_by(|a, b| a.name.cmp(&b.name));
    }

    Ok(HfQuantizationsResponse {
        model_id: model_id.to_string(),
        quantizations,
    })
}
