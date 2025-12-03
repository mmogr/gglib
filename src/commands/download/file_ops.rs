use anyhow::{Result, anyhow};
use std::fs;
use std::path::PathBuf;
use strum_macros::{Display, EnumIter, EnumString};

use super::model_ops::{DownloadConfig, DownloadContext, add_to_database};
use super::python_bridge::{FastDownloadRequest, run_fast_download};
use super::utils::sanitize_model_name;
use reqwest::StatusCode;

/// Represents the quantization type of a GGUF model file.
///
/// This enum provides type-safe handling of quantization types commonly used
/// in GGUF model naming conventions. Use `extract_quantization_from_filename`
/// to parse a filename into this enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumString, Display, EnumIter)]
pub enum Quantization {
    // 1-bit quantizations
    #[strum(serialize = "IQ1_S")]
    Iq1S,
    #[strum(serialize = "IQ1_M")]
    Iq1M,

    // 2-bit quantizations
    #[strum(serialize = "IQ2_XXS")]
    Iq2Xxs,
    #[strum(serialize = "IQ2_XS")]
    Iq2Xs,
    #[strum(serialize = "IQ2_S")]
    Iq2S,
    #[strum(serialize = "IQ2_M")]
    Iq2M,
    #[strum(serialize = "Q2_K_XL")]
    Q2KXl,
    #[strum(serialize = "Q2_K_L")]
    Q2KL,
    #[strum(serialize = "Q2_K")]
    Q2K,

    // 3-bit quantizations
    #[strum(serialize = "IQ3_XXS")]
    Iq3Xxs,
    #[strum(serialize = "IQ3_XS")]
    Iq3Xs,
    #[strum(serialize = "IQ3_M")]
    Iq3M,
    #[strum(serialize = "Q3_K_XL")]
    Q3KXl,
    #[strum(serialize = "Q3_K_L")]
    Q3KL,
    #[strum(serialize = "Q3_K_M")]
    Q3KM,
    #[strum(serialize = "Q3_K_S")]
    Q3KS,

    // 4-bit quantizations
    #[strum(serialize = "IQ4_XS")]
    Iq4Xs,
    #[strum(serialize = "IQ4_NL")]
    Iq4Nl,
    #[strum(serialize = "Q4_K_XL")]
    Q4KXl,
    #[strum(serialize = "Q4_K_L")]
    Q4KL,
    #[strum(serialize = "Q4_K_M")]
    Q4KM,
    #[strum(serialize = "Q4_K_S")]
    Q4KS,
    #[strum(serialize = "Q4_1")]
    Q4_1,
    #[strum(serialize = "Q4_0")]
    Q4_0,
    #[strum(serialize = "MXFP4")]
    Mxfp4,
    #[strum(serialize = "Q4")]
    Q4,

    // 5-bit quantizations
    #[strum(serialize = "Q5_K_XL")]
    Q5KXl,
    #[strum(serialize = "Q5_K_L")]
    Q5KL,
    #[strum(serialize = "Q5_K_M")]
    Q5KM,
    #[strum(serialize = "Q5_K_S")]
    Q5KS,
    #[strum(serialize = "Q5_0")]
    Q5_0,
    #[strum(serialize = "Q5_1")]
    Q5_1,
    #[strum(serialize = "Q5")]
    Q5,

    // 6-bit quantizations
    #[strum(serialize = "Q6_K_XL")]
    Q6KXl,
    #[strum(serialize = "Q6_K_L")]
    Q6KL,
    #[strum(serialize = "Q6_K")]
    Q6K,
    #[strum(serialize = "Q6")]
    Q6,

    // 8-bit quantizations
    #[strum(serialize = "Q8_K_XL")]
    Q8KXl,
    #[strum(serialize = "Q8_0")]
    Q8_0,
    #[strum(serialize = "Q8")]
    Q8,

    // 16-bit and higher precision
    #[strum(serialize = "BF16")]
    Bf16,
    #[strum(serialize = "F16")]
    F16,
    #[strum(serialize = "F32")]
    F32,

    // Special formats
    #[strum(serialize = "imatrix")]
    Imatrix,

    #[strum(serialize = "unknown")]
    Unknown,
}

impl Quantization {
    /// Returns true if this quantization type is unknown.
    pub fn is_unknown(&self) -> bool {
        matches!(self, Quantization::Unknown)
    }
}

/// Pattern table for quantization extraction, ordered by specificity.
/// More specific patterns (longer, more detailed) come before generic ones
/// within each family to ensure correct matching.
const QUANT_PATTERNS: &[(&str, Quantization)] = &[
    // 1-bit quantizations
    ("IQ1_S", Quantization::Iq1S),
    ("IQ1_M", Quantization::Iq1M),
    // 2-bit quantizations (most specific first)
    ("IQ2_XXS", Quantization::Iq2Xxs),
    ("IQ2_XS", Quantization::Iq2Xs),
    ("IQ2_S", Quantization::Iq2S),
    ("IQ2_M", Quantization::Iq2M),
    ("Q2_K_XL", Quantization::Q2KXl),
    ("Q2_K_L", Quantization::Q2KL),
    ("Q2_K", Quantization::Q2K),
    // 3-bit quantizations (most specific first)
    ("IQ3_XXS", Quantization::Iq3Xxs),
    ("IQ3_XS", Quantization::Iq3Xs),
    ("IQ3_M", Quantization::Iq3M),
    ("Q3_K_XL", Quantization::Q3KXl),
    ("Q3_K_L", Quantization::Q3KL),
    ("Q3_K_M", Quantization::Q3KM),
    ("Q3_K_S", Quantization::Q3KS),
    // 4-bit quantizations (most specific first)
    ("IQ4_XS", Quantization::Iq4Xs),
    ("IQ4_NL", Quantization::Iq4Nl),
    ("Q4_K_XL", Quantization::Q4KXl),
    ("Q4_K_L", Quantization::Q4KL),
    ("Q4_K_M", Quantization::Q4KM),
    ("Q4_K_S", Quantization::Q4KS),
    ("Q4_1", Quantization::Q4_1),
    ("Q4_0", Quantization::Q4_0),
    ("MXFP4", Quantization::Mxfp4),
    ("Q4", Quantization::Q4),
    // 5-bit quantizations (most specific first)
    ("Q5_K_XL", Quantization::Q5KXl),
    ("Q5_K_L", Quantization::Q5KL),
    ("Q5_K_M", Quantization::Q5KM),
    ("Q5_K_S", Quantization::Q5KS),
    ("Q5_0", Quantization::Q5_0),
    ("Q5_1", Quantization::Q5_1),
    ("Q5", Quantization::Q5),
    // 6-bit quantizations (most specific first)
    ("Q6_K_XL", Quantization::Q6KXl),
    ("Q6_K_L", Quantization::Q6KL),
    ("Q6_K", Quantization::Q6K),
    ("Q6", Quantization::Q6),
    // 8-bit quantizations (most specific first)
    ("Q8_K_XL", Quantization::Q8KXl),
    ("Q8_0", Quantization::Q8_0),
    ("Q8", Quantization::Q8),
    // 16-bit and higher precision
    ("BF16", Quantization::Bf16),
    ("FP16", Quantization::F16),
    ("F16", Quantization::F16),
    ("FP32", Quantization::F32),
    ("F32", Quantization::F32),
    // Special formats
    ("IMATRIX", Quantization::Imatrix),
];

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
            // For sharded models, use the first shard path (llama-server requires it).
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

    // Add to database if requested
    if context.add_to_db {
        // For sharded models, use the first shard path (llama-server requires it).
        // For non-sharded models, use the downloaded file path.
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
/// Returns a `Quantization` enum variant representing the detected type,
/// or `Quantization::Unknown` if no recognized pattern is found.
///
/// # Examples
///
/// ```rust
/// use gglib::commands::download::file_ops::{extract_quantization_from_filename, Quantization};
///
/// assert_eq!(extract_quantization_from_filename("model-Q4_K_M.gguf"), Quantization::Q4KM);
/// assert_eq!(extract_quantization_from_filename("llama-F16.gguf"), Quantization::F16);
/// assert_eq!(extract_quantization_from_filename("unknown.gguf"), Quantization::Unknown);
/// ```
pub fn extract_quantization_from_filename(filename: &str) -> Quantization {
    let upper = filename.to_uppercase();
    QUANT_PATTERNS
        .iter()
        .find(|(pattern, _)| upper.contains(pattern))
        .map(|(_, q)| *q)
        .unwrap_or(Quantization::Unknown)
}

/// Convert a shard filename to the first shard filename.
///
/// For split GGUF files with patterns like "model-00002-of-00003.gguf",
/// this returns "model-00001-of-00003.gguf". This is needed because
/// llama-server requires the first shard to be specified when loading
/// split models.
///
/// If the filename is not a shard pattern, returns it unchanged.
pub fn get_first_shard_filename(filename: &str) -> String {
    use regex::Regex;
    // Match pattern like "-00002-of-00003" and replace with "-00001-of-00003"
    let re = Regex::new(r"-(\d+)-of-(\d+)").unwrap();
    if let Some(caps) = re.captures(filename) {
        let total = &caps[2];
        // Keep the same width for the first shard number
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
