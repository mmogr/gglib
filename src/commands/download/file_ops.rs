use anyhow::{Result, anyhow};
use std::fs;
use std::path::PathBuf;

use super::model_ops::{DownloadConfig, DownloadContext, add_to_database};
use super::python_bridge::{FastDownloadRequest, run_fast_download};
use super::utils::sanitize_model_name;
use reqwest::StatusCode;

/// Callback for download progress: (downloaded_bytes, total_bytes)
pub type ProgressCallback = Box<dyn Fn(u64, u64) + Send + Sync>;

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

/// Download a specific file and handle storage
pub async fn download_specific_file(
    filename: &str,
    commit_sha: &str,
    context: &DownloadContext<'_>,
) -> Result<()> {
    // Create model directory
    let model_dir = context
        .models_dir
        .join(sanitize_model_name(context.model_id));
    if !model_dir.exists() {
        fs::create_dir_all(&model_dir)?;
    }

    let local_path = model_dir.join(filename);

    // Ensure parent directories exist for nested paths (e.g. sharded files)
    #[allow(clippy::collapsible_if)]
    if let Some(parent) = local_path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)?;
        }
    }

    // Check if file already exists and not forcing
    if local_path.exists() && !context.force {
        println!(
            "File already exists: {} (use --force to overwrite)",
            local_path.display()
        );
        if context.add_to_db {
            let quant = extract_quantization_from_filename(filename);
            add_to_database(context.model_id, commit_sha, &local_path, quant).await?;
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
    };

    run_fast_download(&fast_request).await?;
    println!("⚡ Downloaded via fast helper: {}", filename);
    println!(
        "✓ Successfully downloaded {} to {}",
        filename,
        local_path.display()
    );

    // Add to database if requested
    if context.add_to_db {
        let quant = extract_quantization_from_filename(filename);
        add_to_database(context.model_id, commit_sha, &local_path, quant).await?;
    }

    Ok(())
}

/// Download sharded GGUF files (multi-part files)
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

    // Create model directory
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

        if let Some(parent) = local_path.parent().filter(|parent| !parent.exists()) {
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

    // Add to database if requested
    // For sharded files, we'll add the first file as the primary entry with a note about sharding
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

/// Try to download files using various patterns
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

/// Extract quantization type from filename
///
/// Analyzes a filename to determine the quantization type based on common
/// patterns used in GGUF model naming conventions.
///
/// # Arguments
///
/// * `filename` - The filename to analyze
///
/// # Returns
///
/// Returns a string slice representing the detected quantization type,
/// or "unknown" if no recognized pattern is found.
///
/// # Examples
///
/// ```rust
/// use gglib::commands::download::extract_quantization_from_filename;
///
/// assert_eq!(extract_quantization_from_filename("model-Q4_K_M.gguf"), "Q4_K_M");
/// assert_eq!(extract_quantization_from_filename("llama-F16.gguf"), "F16");
/// assert_eq!(extract_quantization_from_filename("unknown.gguf"), "unknown");
/// ```
pub fn extract_quantization_from_filename(filename: &str) -> &str {
    let filename_upper = filename.to_uppercase();

    // 1-bit quantizations
    if filename_upper.contains("IQ1_S") {
        "IQ1_S"
    } else if filename_upper.contains("IQ1_M") {
        "IQ1_M"
    }
    // 2-bit quantizations
    else if filename_upper.contains("IQ2_XXS") {
        "IQ2_XXS"
    } else if filename_upper.contains("IQ2_XS") {
        "IQ2_XS"
    } else if filename_upper.contains("IQ2_S") {
        "IQ2_S"
    } else if filename_upper.contains("IQ2_M") {
        "IQ2_M"
    } else if filename_upper.contains("Q2_K_XL") {
        "Q2_K_XL"
    } else if filename_upper.contains("Q2_K_L") {
        "Q2_K_L"
    } else if filename_upper.contains("Q2_K") {
        "Q2_K"
    }
    // 3-bit quantizations
    else if filename_upper.contains("IQ3_XXS") {
        "IQ3_XXS"
    } else if filename_upper.contains("IQ3_XS") {
        "IQ3_XS"
    } else if filename_upper.contains("IQ3_M") {
        "IQ3_M"
    } else if filename_upper.contains("Q3_K_XL") {
        "Q3_K_XL"
    } else if filename_upper.contains("Q3_K_L") {
        "Q3_K_L"
    } else if filename_upper.contains("Q3_K_M") {
        "Q3_K_M"
    } else if filename_upper.contains("Q3_K_S") {
        "Q3_K_S"
    }
    // 4-bit quantizations
    else if filename_upper.contains("IQ4_XS") {
        "IQ4_XS"
    } else if filename_upper.contains("IQ4_NL") {
        "IQ4_NL"
    } else if filename_upper.contains("Q4_K_XL") {
        "Q4_K_XL"
    } else if filename_upper.contains("Q4_K_L") {
        "Q4_K_L"
    } else if filename_upper.contains("Q4_K_M") {
        "Q4_K_M"
    } else if filename_upper.contains("Q4_K_S") {
        "Q4_K_S"
    } else if filename_upper.contains("Q4_1") {
        "Q4_1"
    } else if filename_upper.contains("Q4_0") {
        "Q4_0"
    } else if filename_upper.contains("MXFP4") {
        "MXFP4"
    } else if filename_upper.contains("Q4") {
        "Q4"
    }
    // 5-bit quantizations
    else if filename_upper.contains("Q5_K_XL") {
        "Q5_K_XL"
    } else if filename_upper.contains("Q5_K_L") {
        "Q5_K_L"
    } else if filename_upper.contains("Q5_K_M") {
        "Q5_K_M"
    } else if filename_upper.contains("Q5_K_S") {
        "Q5_K_S"
    } else if filename_upper.contains("Q5_0") {
        "Q5_0"
    } else if filename_upper.contains("Q5_1") {
        "Q5_1"
    } else if filename_upper.contains("Q5") {
        "Q5"
    }
    // 6-bit quantizations
    else if filename_upper.contains("Q6_K_XL") {
        "Q6_K_XL"
    } else if filename_upper.contains("Q6_K_L") {
        "Q6_K_L"
    } else if filename_upper.contains("Q6_K") {
        "Q6_K"
    } else if filename_upper.contains("Q6") {
        "Q6"
    }
    // 8-bit quantizations
    else if filename_upper.contains("Q8_K_XL") {
        "Q8_K_XL"
    } else if filename_upper.contains("Q8_0") {
        "Q8_0"
    } else if filename_upper.contains("Q8") {
        "Q8"
    }
    // 16-bit quantizations
    else if filename_upper.contains("BF16") {
        "BF16"
    } else if filename_upper.contains("F16") || filename_upper.contains("FP16") {
        "F16"
    } else if filename_upper.contains("F32") || filename_upper.contains("FP32") {
        "F32"
    }
    // Special formats
    else if filename_upper.contains("IMATRIX") {
        "imatrix"
    } else {
        "unknown"
    }
}
