#![allow(clippy::collapsible_if)]

use anyhow::{Result, anyhow};
use chrono::Utc;
use hf_hub::api::sync::Api;
use reqwest;
use serde_json;
use std::io::Write;
use std::path::{Path, PathBuf};
use tokio_util::sync::CancellationToken;

use super::api::create_hf_api;
use super::file_ops::{
    ProgressCallback, download_sharded_files, download_specific_file,
    extract_quantization_from_filename, try_download_with_patterns,
};
use super::utils::get_models_directory;
use crate::models::Gguf;
use crate::services::core::HuggingFaceService;
use crate::services::core::download_service::PidStorage;

/// Configuration for downloading sharded files
pub struct DownloadConfig<'a> {
    pub model_id: &'a str,
    pub commit_sha: &'a str,
    pub models_dir: &'a Path,
    pub force: bool,
    pub add_to_db: bool,
    pub quantization: &'a str,
}

/// Session-specific options shared across download helpers
#[derive(Default)]
pub struct SessionOptions<'a> {
    pub auth_token: Option<String>,
    pub progress_callback: Option<&'a ProgressCallback>,
    /// Cancellation token for external cancellation (GUI/service layer)
    pub cancel_token: Option<CancellationToken>,
    /// Optional PID storage for synchronous process termination on app shutdown
    pub pid_storage: Option<PidStorage>,
    /// Key used to identify this download in the PID storage
    pub pid_key: Option<String>,
}

impl SessionOptions<'_> {
    pub fn token(&self) -> Option<&str> {
        self.auth_token.as_deref()
    }
}

/// Consolidated context passed through download operations
pub struct DownloadContext<'a> {
    pub model_id: &'a str,
    pub quantization: Option<&'a str>,
    pub models_dir: &'a Path,
    pub force: bool,
    pub add_to_db: bool,
    pub session: SessionOptions<'a>,
    /// For sharded models: the path to the first shard file (used for database registration).
    /// llama-server requires the first shard to be specified when loading split models.
    pub first_shard_path: Option<PathBuf>,
}

/// Download a specific model with optional quantization filter
pub async fn download_model(api: &Api, context: DownloadContext<'_>) -> Result<()> {
    println!("Downloading {} from HuggingFace Hub...", context.model_id);

    let repo = api.repo(hf_hub::Repo::with_revision(
        context.model_id.to_string(),
        hf_hub::RepoType::Model,
        "main".to_string(),
    ));

    // Get repository info
    let repo_info = repo
        .info()
        .map_err(|e| anyhow!("Failed to get repo info: {}", e))?;
    let commit_sha = repo_info.sha.clone();

    println!("Found repository, commit SHA: {}", commit_sha);

    if let Some(quant) = context.quantization {
        println!(
            "Looking for {} quantization using HuggingFace API...",
            quant
        );

        let quant_upper = quant.to_uppercase();

        // Use HuggingFaceService for consistent URL construction (DRY)
        let api_url = HuggingFaceService::build_tree_url(context.model_id, None);

        match reqwest::get(&api_url).await {
            Ok(response) => {
                if response.status().is_success() {
                    match response.text().await {
                        Ok(json_text) => {
                            match serde_json::from_str::<serde_json::Value>(&json_text) {
                                Ok(data) => {
                                    if let Some(files) = data.as_array() {
                                        // First, collect all GGUF files and group by quantization
                                        let mut quantization_files: std::collections::HashMap<
                                            String,
                                            Vec<String>,
                                        > = std::collections::HashMap::new();

                                        // 1) Handle top-level files
                                        for file in files {
                                            if let Some(filename) =
                                                file.get("path").and_then(|v| v.as_str())
                                            {
                                                let entry_type = file
                                                    .get("type")
                                                    .and_then(|v| v.as_str())
                                                    .unwrap_or("file");

                                                // Direct GGUF files at repo root
                                                if entry_type == "file"
                                                    && filename.ends_with(".gguf")
                                                {
                                                    let file_quant =
                                                        extract_quantization_from_filename(
                                                            filename,
                                                        );
                                                    if file_quant.to_uppercase() == quant_upper {
                                                        quantization_files
                                                            .entry(file_quant.to_uppercase())
                                                            .or_default()
                                                            .push(filename.to_string());
                                                    }
                                                }

                                                // Sharded GGUF files live in per-quant directories
                                                if entry_type == "directory"
                                                    && filename
                                                        .to_uppercase()
                                                        .contains(&quant_upper)
                                                {
                                                    let sub_api_url =
                                                        HuggingFaceService::build_tree_url(
                                                            context.model_id,
                                                            Some(filename),
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
                                                                        for sub_file in sub_files {
                                                                            if let Some(sub_path) =
                                                                                sub_file
                                                                                    .get("path")
                                                                                    .and_then(|v| {
                                                                                        v.as_str()
                                                                                    })
                                                                            {
                                                                                if sub_path
                                                                                    .ends_with(
                                                                                        ".gguf",
                                                                                    )
                                                                                {
                                                                                    let sub_quant = extract_quantization_from_filename(sub_path);
                                                                                    if sub_quant.to_uppercase()
                                                                                        == quant_upper
                                                                                    {
                                                                                        quantization_files
                                                                                            .entry(sub_quant.to_uppercase())
                                                                                            .or_default()
                                                                                            .push(sub_path.to_string());
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

                                        if let Some(matching_files) =
                                            quantization_files.get(&quant_upper)
                                        {
                                            if matching_files.is_empty() {
                                                return Err(anyhow!(
                                                    "No GGUF file found for quantization '{}'. Use --list-quants to see available options.",
                                                    quant
                                                ));
                                            }

                                            // Sort sharded files to ensure proper order (00001, 00002, etc.)
                                            let mut sorted_files = matching_files.clone();
                                            sorted_files.sort();

                                            if sorted_files.len() == 1 {
                                                // Single file
                                                println!("✓ Found file: {}", sorted_files[0]);
                                                return download_specific_file(
                                                    &sorted_files[0],
                                                    &commit_sha,
                                                    &context,
                                                )
                                                .await;
                                            } else {
                                                // Multiple sharded files
                                                println!(
                                                    "✓ Found {} sharded files for quantization {}",
                                                    sorted_files.len(),
                                                    quant
                                                );
                                                for (i, filename) in sorted_files.iter().enumerate()
                                                {
                                                    println!("  Part {}: {}", i + 1, filename);
                                                }

                                                return download_sharded_files(
                                                    &sorted_files,
                                                    DownloadConfig {
                                                        model_id: context.model_id,
                                                        commit_sha: &commit_sha,
                                                        models_dir: context.models_dir,
                                                        force: context.force,
                                                        add_to_db: context.add_to_db,
                                                        quantization: quant,
                                                    },
                                                    &context,
                                                )
                                                .await;
                                            }
                                        }
                                        return Err(anyhow!(
                                            "No GGUF file found for quantization '{}'. Use --list-quants to see available options.",
                                            quant
                                        ));
                                    }
                                }
                                Err(e) => {
                                    println!("Failed to parse API response: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            println!("Failed to read API response: {}", e);
                        }
                    }
                } else {
                    println!("API request failed with status: {}", response.status());
                }
            }
            Err(e) => {
                println!("Failed to make API request: {}", e);
            }
        }

        // Only fall back to pattern matching if API completely fails
        println!("API approach failed, falling back to pattern matching...");
        return try_download_with_patterns(quant, &commit_sha, &context).await;
    } else {
        Err(anyhow!(
            "Please specify a quantization. Use --list-quants to see available options."
        ))
    }
}

/// Add downloaded model to database
pub async fn add_to_database(
    model_id: &str,
    commit_sha: &str,
    file_path: &Path,
    quantization: &str,
) -> Result<()> {
    use crate::services::{AppCore, database};
    use crate::utils::validation;

    // Parse GGUF metadata from the downloaded file to extract param_count_b and other info
    // If parsing fails, log a warning and fall back to defaults
    let file_path_str = file_path
        .to_str()
        .ok_or_else(|| anyhow!("Invalid file path"))?;

    let gguf_metadata = match validation::validate_and_parse_gguf(file_path_str) {
        Ok(metadata) => {
            println!("✓ Parsed GGUF metadata from downloaded file");
            Some(metadata)
        }
        Err(e) => {
            eprintln!(
                "Warning: Failed to parse GGUF metadata, using defaults: {}",
                e
            );
            None
        }
    };

    // Extract param_count_b from metadata, fall back to 0.0
    let param_count_b = gguf_metadata
        .as_ref()
        .and_then(|m| m.param_count_b)
        .unwrap_or(0.0);

    let mut model = Gguf::new(
        model_id.to_string(),
        file_path.to_path_buf(),
        param_count_b,
        Utc::now(),
    );

    // Use extracted metadata where available, with fallbacks
    model.quantization = gguf_metadata
        .as_ref()
        .and_then(|m| m.quantization.clone())
        .or_else(|| Some(quantization.to_string()));
    model.architecture = gguf_metadata.as_ref().and_then(|m| m.architecture.clone());
    model.context_length = gguf_metadata.as_ref().and_then(|m| m.context_length);
    if let Some(ref meta) = gguf_metadata {
        model.metadata = meta.metadata.clone();
    }
    model.hf_repo_id = Some(model_id.to_string());
    model.hf_commit_sha = Some(commit_sha.to_string());
    model.hf_filename = Some(file_path.file_name().unwrap().to_string_lossy().to_string());
    model.download_date = Some(Utc::now());

    println!("Adding model to database...");

    let pool = database::setup_database().await?;
    let core = AppCore::new(pool);
    core.models().add(&model).await?;

    println!("✓ Successfully added model to database:");
    println!("  Model: {}", model.name);
    println!("  File: {}", model.file_path.display());
    println!("  Parameters: {:.1}B", model.param_count_b);
    println!("  Quantization: {:?}", model.quantization);
    println!("  Architecture: {:?}", model.architecture);
    println!("  HF Repo: {:?}", model.hf_repo_id);

    Ok(())
}

/// Check if a model needs updates
pub async fn check_model_update(model: &Gguf, hf_repo: &str) -> Result<()> {
    println!("Checking updates for: {}", model.name);

    let models_dir = get_models_directory()?;
    let api = create_hf_api(None, &models_dir)?;
    let repo = api.repo(hf_hub::Repo::with_revision(
        hf_repo.to_string(),
        hf_hub::RepoType::Model,
        "main".to_string(),
    ));

    match repo.info() {
        Ok(repo_info) => {
            let latest_sha = repo_info.sha;

            if let Some(stored_sha) = &model.hf_commit_sha {
                if *stored_sha == latest_sha {
                    println!("  ✓ Model is up to date (SHA: {})", &latest_sha[..8]);
                } else {
                    println!("  🔄 Update available!");
                    println!("    Current SHA: {}", &stored_sha[..8]);
                    println!("    Latest SHA:  {}", &latest_sha[..8]);
                    println!(
                        "    Use: gglib update-model {} to update",
                        model.id.unwrap_or(0)
                    );
                }
            } else {
                println!("  ⚠️  No commit SHA stored, cannot check for updates");
            }
        }
        Err(e) => {
            println!("  ❌ Failed to check repository: {}", e);
        }
    }

    Ok(())
}

/// Handle check updates command
pub async fn handle_check_updates(model_id: Option<u32>, all: bool) -> Result<()> {
    use crate::services::{AppCore, database};

    let pool = database::setup_database().await?;
    let core = AppCore::new(pool);

    if all {
        println!("Checking updates for all models...");
        let models = core.models().list().await?;

        if models.is_empty() {
            println!("No models found in database.");
            return Ok(());
        }

        for model in models {
            if let Some(hf_repo) = &model.hf_repo_id {
                check_model_update(&model, hf_repo).await?;
            } else {
                println!(
                    "Model '{}' is not from HuggingFace, skipping update check.",
                    model.name
                );
            }
        }
    } else if let Some(id) = model_id {
        match core.models().get_by_id(id).await {
            Ok(model) => {
                if let Some(hf_repo) = &model.hf_repo_id {
                    check_model_update(&model, hf_repo).await?;
                } else {
                    println!(
                        "Model '{}' is not from HuggingFace, cannot check for updates.",
                        model.name
                    );
                }
            }
            Err(_) => {
                println!("Model with ID {} not found.", id);
            }
        }
    } else {
        println!("Please specify --model-id <ID> or --all to check for updates.");
    }

    Ok(())
}

/// Handle update model command  
pub async fn handle_update_model(model_id: u32, force: bool) -> Result<()> {
    use crate::services::{AppCore, database};

    let pool = database::setup_database().await?;
    let core = AppCore::new(pool);

    match core.models().get_by_id(model_id).await {
        Ok(model) => {
            if let Some(hf_repo) = &model.hf_repo_id {
                if let Some(quantization) = &model.quantization {
                    println!("Updating model: {}", model.name);

                    if !force {
                        print!("This will re-download the model. Continue? [y/N]: ");
                        std::io::stdout().flush()?;
                        let mut input = String::new();
                        std::io::stdin().read_line(&mut input)?;
                        if !input.trim().to_lowercase().starts_with('y') {
                            println!("Update cancelled.");
                            return Ok(());
                        }
                    }

                    // Get models directory
                    let models_dir = get_models_directory()?;

                    // Re-download the model
                    let api = create_hf_api(None, &models_dir)?;
                    let context = DownloadContext {
                        model_id: hf_repo,
                        quantization: Some(quantization.as_str()),
                        models_dir: &models_dir,
                        force: true,
                        add_to_db: true,
                        session: SessionOptions::default(),
                        first_shard_path: None, // Update path handles sharding via download_sharded_files
                    };

                    download_model(&api, context).await?;

                    println!("✓ Model updated successfully!");
                } else {
                    println!("Cannot update model: no quantization information stored");
                }
            } else {
                println!("Cannot update model: not downloaded from HuggingFace");
            }
        }
        Err(_) => {
            println!("Model with ID {} not found.", model_id);
        }
    }

    Ok(())
}
