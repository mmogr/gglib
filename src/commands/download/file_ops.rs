//! File operations for download commands.
//!
//! This module provides utilities for downloading specific files and sharded
//! GGUF models from HuggingFace repositories.

use std::fs;
use std::path::PathBuf;

use anyhow::{Result, anyhow};
use reqwest::StatusCode;

use super::model_ops::{DownloadConfig, DownloadContext, add_to_database};
use super::python_bridge::{FastDownloadRequest, run_fast_download};
use super::utils::sanitize_model_name;

// Re-export the canonical Quantization from domain
pub use crate::download::domain::types::Quantization;

/// Callback for download progress: (downloaded_bytes, total_bytes)
pub type ProgressCallback = Box<dyn Fn(u64, u64) + Send + Sync>;

// ============================================================================
// File Existence Check
// ============================================================================

async fn remote_file_exists(
    repo_id: &str,
    revision: &str,
    filename: &str,
    token: Option<&str>,
) -> Result<bool> {
    let url = format!(
        "https://huggingface.co/{}/resolve/{}/{}",
        repo_id, revision, filename
    );
    let client = reqwest::Client::new();
    let mut request = client.head(&url);
    if let Some(t) = token {
        request = request.header("Authorization", format!("Bearer {}", t));
    }
    let response = request.send().await?;
    match response.status() {
        StatusCode::OK => Ok(true),
        StatusCode::NOT_FOUND => Ok(false),
        status => Err(anyhow!("Failed to probe {}: {}", filename, status)),
    }
}

// ============================================================================
// Download Functions
// ============================================================================

/// Download a specific file and handle storage.
pub async fn download_specific_file(
    filename: &str,
    commit_sha: &str,
    context: &DownloadContext<'_>,
) -> Result<()> {
    let model_dir = context
        .models_dir
        .join(sanitize_model_name(context.model_id));
    if !model_dir.exists() {
        fs::create_dir_all(&model_dir)?;
    }

    let local_path = model_dir.join(filename);

    // Ensure parent directories exist for nested paths (e.g. sharded files)
    if let Some(parent) = local_path.parent().filter(|p| !p.exists()) {
        fs::create_dir_all(parent)?;
    }

    // Check if file already exists and not forcing
    if local_path.exists() && !context.force {
        println!(
            "File already exists: {} (use --force to overwrite)",
            local_path.display()
        );
        if context.add_to_db {
            let db_path = context.first_shard_path.as_ref().unwrap_or(&local_path);
            let quant = extract_quantization_from_filename(
                db_path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or(filename),
            );
            add_to_database(context.model_id, commit_sha, db_path, &quant.to_string()).await?;
        }
        return Ok(());
    }

    println!("Downloading: {}", filename);

    let fast_files = vec![filename.to_string()];
    let fast_request = FastDownloadRequest {
        repo_id: context.model_id,
        revision: commit_sha,
        repo_type: "model",
        destination: &model_dir,
        files: &fast_files,
        token: context.session.token(),
        force: context.force,
        progress: context.session.progress_callback,
        cancel_token: context.session.cancel_token.clone(),
        pid_storage: context.session.pid_storage.clone(),
        pid_key: context.session.pid_key.clone(),
    };

    run_fast_download(&fast_request).await?;
    println!("⚡ Downloaded via fast helper: {}", filename);
    println!(
        "✓ Successfully downloaded {} to {}",
        filename,
        local_path.display()
    );

    if context.add_to_db {
        let db_path = context.first_shard_path.as_ref().unwrap_or(&local_path);
        let quant = extract_quantization_from_filename(
            db_path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(filename),
        );
        add_to_database(context.model_id, commit_sha, db_path, &quant.to_string()).await?;
    }

    Ok(())
}

/// Download sharded GGUF files (multi-part files).
pub async fn download_sharded_files(
    filenames: &[String],
    config: DownloadConfig<'_>,
    context: &DownloadContext<'_>,
) -> Result<()> {
    println!(
        "Downloading {} sharded files for {} quantization...",
        filenames.len(),
        config.quantization
    );

    let model_dir = config.models_dir.join(sanitize_model_name(config.model_id));
    if !model_dir.exists() {
        fs::create_dir_all(&model_dir)?;
    }

    struct PendingPart {
        name: String,
        local_path: PathBuf,
        ordinal: usize,
    }

    let mut downloaded_files = Vec::new();
    let mut total_size = 0u64;
    let total_parts = filenames.len();
    let mut pending_parts: Vec<PendingPart> = Vec::new();

    for (i, filename) in filenames.iter().enumerate() {
        let local_path = model_dir.join(filename);

        if let Some(parent) = local_path.parent().filter(|p| !p.exists()) {
            fs::create_dir_all(parent)?;
        }

        if local_path.exists() && !config.force {
            println!(
                "Part {} already exists: {} (use --force to overwrite)",
                i + 1,
                local_path.display()
            );
            downloaded_files.push(local_path.clone());
            if let Ok(metadata) = std::fs::metadata(&local_path) {
                total_size += metadata.len();
            }
            continue;
        }

        pending_parts.push(PendingPart {
            name: filename.clone(),
            local_path: local_path.clone(),
            ordinal: i + 1,
        });
    }

    if !pending_parts.is_empty() {
        println!(
            "Attempting fast download for {} remaining parts via hf_xet...",
            pending_parts.len()
        );
        let pending_names: Vec<String> = pending_parts.iter().map(|p| p.name.clone()).collect();
        let fast_request = FastDownloadRequest {
            repo_id: config.model_id,
            revision: config.commit_sha,
            repo_type: "model",
            destination: &model_dir,
            files: &pending_names,
            token: context.session.token(),
            force: context.force,
            progress: context.session.progress_callback,
            cancel_token: context.session.cancel_token.clone(),
            pid_storage: context.session.pid_storage.clone(),
            pid_key: context.session.pid_key.clone(),
        };

        run_fast_download(&fast_request).await?;
        println!(
            "⚡ Fast helper downloaded {} pending parts",
            pending_names.len()
        );

        for part in pending_parts.drain(..) {
            if let Ok(metadata) = std::fs::metadata(&part.local_path) {
                total_size += metadata.len();
                println!(
                    "✓ Part {}/{} downloaded: {} ({:.1} MB)",
                    part.ordinal,
                    total_parts,
                    part.name,
                    metadata.len() as f64 / 1_048_576.0
                );
            } else {
                println!(
                    "✓ Part {}/{} downloaded: {}",
                    part.ordinal, total_parts, part.name
                );
            }
            downloaded_files.push(part.local_path);
        }
    }

    println!(
        "✅ Successfully downloaded all {} parts for {} quantization (Total: {:.1} MB)",
        filenames.len(),
        config.quantization,
        total_size as f64 / 1_048_576.0
    );

    if config.add_to_db && !downloaded_files.is_empty() {
        println!("Adding sharded model to database...");
        let primary_file = &downloaded_files[0];
        let quant_with_note = format!(
            "{} (sharded: {} parts)",
            config.quantization,
            filenames.len()
        );
        add_to_database(
            config.model_id,
            config.commit_sha,
            primary_file,
            &quant_with_note,
        )
        .await?;
    }

    Ok(())
}

/// Try to download files using various patterns (fallback when resolver fails).
pub async fn try_download_with_patterns(
    quant: &str,
    commit_sha: &str,
    context: &DownloadContext<'_>,
) -> Result<()> {
    let full_model_name = context.model_id.split('/').next_back().unwrap_or("model");
    let model_name = full_model_name
        .strip_suffix("-GGUF")
        .unwrap_or(full_model_name);

    let common_patterns = [
        format!("{}-{}.gguf", model_name, quant),
        format!("{}-{}.gguf", model_name, quant.to_uppercase()),
        format!("{}-{}.gguf", model_name, quant.to_lowercase()),
    ];

    println!(
        "Trying {} filename patterns for quantization '{}'...",
        common_patterns.len(),
        quant
    );

    for (i, pattern) in common_patterns.iter().enumerate() {
        println!(
            "  [{}/{}] Trying: {}",
            i + 1,
            common_patterns.len(),
            pattern
        );

        match remote_file_exists(
            context.model_id,
            commit_sha,
            pattern,
            context.session.token(),
        )
        .await
        {
            Ok(true) => {
                println!("Found file via pattern: {}", pattern);
                return download_specific_file(pattern, commit_sha, context).await;
            }
            Ok(false) => continue,
            Err(err) => {
                println!("  ⚠️  Failed to probe {}: {}", pattern, err);
                continue;
            }
        }
    }

    Err(anyhow!("No GGUF file found for quantization: {}", quant))
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Extract quantization type from filename.
///
/// Delegates to the canonical `Quantization::from_filename` in domain/types.
pub fn extract_quantization_from_filename(filename: &str) -> Quantization {
    Quantization::from_filename(filename)
}

/// Convert a shard filename to the first shard filename.
///
/// For split GGUF files with patterns like "model-00002-of-00003.gguf",
/// this returns "model-00001-of-00003.gguf". This is needed because
/// llama-server requires the first shard to be specified when loading
/// split models.
pub fn get_first_shard_filename(filename: &str) -> String {
    use regex::Regex;
    let re = Regex::new(r"-(\d+)-of-(\d+)").unwrap();
    if let Some(caps) = re.captures(filename) {
        let total = &caps[2];
        let width = caps[1].len();
        let first_shard = format!("{:0>width$}", 1, width = width);
        re.replace(filename, format!("-{}-of-{}", first_shard, total))
            .to_string()
    } else {
        filename.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_first_shard_filename_second_of_two() {
        let result = get_first_shard_filename(
            "UD-Q6_K_XL/Qwen3-Next-80B-A3B-Instruct-UD-Q6_K_XL-00002-of-00002.gguf",
        );
        assert_eq!(
            result,
            "UD-Q6_K_XL/Qwen3-Next-80B-A3B-Instruct-UD-Q6_K_XL-00001-of-00002.gguf"
        );
    }

    #[test]
    fn test_get_first_shard_filename_third_of_five() {
        let result = get_first_shard_filename("model-00003-of-00005.gguf");
        assert_eq!(result, "model-00001-of-00005.gguf");
    }

    #[test]
    fn test_get_first_shard_filename_already_first() {
        let result = get_first_shard_filename("model-00001-of-00003.gguf");
        assert_eq!(result, "model-00001-of-00003.gguf");
    }

    #[test]
    fn test_get_first_shard_filename_non_sharded() {
        let result = get_first_shard_filename("model-Q4_K_M.gguf");
        assert_eq!(result, "model-Q4_K_M.gguf");
    }

    #[test]
    fn test_get_first_shard_filename_with_directory() {
        let result = get_first_shard_filename("Q4_K_M/model-00005-of-00010.gguf");
        assert_eq!(result, "Q4_K_M/model-00001-of-00010.gguf");
    }
}
