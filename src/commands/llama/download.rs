//! Pre-built llama.cpp binary download support.
//!
//! This module handles downloading pre-built llama.cpp binaries from GitHub releases
//! for users running pre-built gglib binaries (not building from source).
//!
//! Platform support:
//! - macOS ARM64: Metal-enabled pre-built binaries
//! - macOS x64: Metal-enabled pre-built binaries
//! - Windows x64: CUDA-enabled pre-built binaries
//! - Linux: No pre-built (must build from source for CUDA support)

use anyhow::{Context, Result, bail};
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Client;
use serde::Deserialize;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;

use crate::utils::paths::{get_gglib_data_dir, get_llama_cli_path, get_llama_server_path};

/// GitHub API response for a release
#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    assets: Vec<GitHubAsset>,
}

/// GitHub API response for a release asset
#[derive(Debug, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

/// Result of checking pre-built binary availability
#[derive(Debug)]
pub enum PrebuiltAvailability {
    /// Pre-built binaries are available for this platform
    Available {
        /// The asset filename pattern to download
        asset_pattern: String,
        /// Description for user-facing messages
        description: String,
    },
    /// Pre-built binaries are not available (must build from source)
    NotAvailable {
        /// Reason why pre-built is not available
        reason: String,
    },
}

/// Check if pre-built llama.cpp binaries are available for the current platform.
///
/// Returns `Available` with asset pattern for macOS (Metal) and Windows (CUDA).
/// Returns `NotAvailable` for Linux (no CUDA pre-built available).
pub fn check_prebuilt_availability() -> PrebuiltAvailability {
    #[cfg(target_os = "macos")]
    {
        #[cfg(target_arch = "aarch64")]
        {
            PrebuiltAvailability::Available {
                asset_pattern: "bin-macos-arm64.zip".to_string(),
                description: "macOS ARM64 (Metal)".to_string(),
            }
        }
        #[cfg(target_arch = "x86_64")]
        {
            PrebuiltAvailability::Available {
                asset_pattern: "bin-macos-x64.zip".to_string(),
                description: "macOS x64 (Metal)".to_string(),
            }
        }
        #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
        {
            PrebuiltAvailability::NotAvailable {
                reason: "Unsupported macOS architecture".to_string(),
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        #[cfg(target_arch = "x86_64")]
        {
            PrebuiltAvailability::Available {
                asset_pattern: "cudart-llama-bin-win-cuda-12.4-x64.zip".to_string(),
                description: "Windows x64 (CUDA 12.4)".to_string(),
            }
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            PrebuiltAvailability::NotAvailable {
                reason: "Unsupported Windows architecture".to_string(),
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        // Linux pre-built binaries don't include CUDA support,
        // and gglib only supports CUDA for GPU acceleration on Linux.
        // Users must build from source to get CUDA support.
        PrebuiltAvailability::NotAvailable {
            reason: "Linux pre-built binaries don't include CUDA support. Building from source is required for GPU acceleration.".to_string(),
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        PrebuiltAvailability::NotAvailable {
            reason: "Unsupported operating system".to_string(),
        }
    }
}

/// Fetch the latest llama.cpp release information from GitHub.
async fn fetch_latest_release(client: &Client) -> Result<GitHubRelease> {
    let url = "https://api.github.com/repos/ggml-org/llama.cpp/releases/latest";

    let response = client
        .get(url)
        .header("User-Agent", "gglib")
        .header("Accept", "application/vnd.github.v3+json")
        .send()
        .await
        .context("Failed to fetch llama.cpp releases from GitHub")?;

    if !response.status().is_success() {
        bail!(
            "GitHub API returned error: {} {}",
            response.status(),
            response.text().await.unwrap_or_default()
        );
    }

    let release: GitHubRelease = response
        .json()
        .await
        .context("Failed to parse GitHub release response")?;

    Ok(release)
}

/// Find the matching asset for our platform in a release.
fn find_platform_asset<'a>(
    release: &'a GitHubRelease,
    asset_pattern: &str,
) -> Option<&'a GitHubAsset> {
    release
        .assets
        .iter()
        .find(|asset| asset.name.contains(asset_pattern))
}

/// Download a file with progress bar.
async fn download_with_progress(client: &Client, url: &str, dest: &Path) -> Result<()> {
    let response = client
        .get(url)
        .header("User-Agent", "gglib")
        .send()
        .await
        .context("Failed to start download")?;

    if !response.status().is_success() {
        bail!("Download failed: HTTP {}", response.status());
    }

    let total_size = response.content_length().unwrap_or(0);

    let pb = ProgressBar::new(total_size);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec})")
            .unwrap()
            .progress_chars("█▓░"),
    );

    // Ensure parent directory exists
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).context("Failed to create download directory")?;
    }

    let mut file = File::create(dest).context("Failed to create download file")?;

    let mut downloaded: u64 = 0;
    let mut stream = response.bytes_stream();

    use futures_util::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("Error reading download stream")?;
        file.write_all(&chunk)
            .context("Error writing to download file")?;
        downloaded += chunk.len() as u64;
        pb.set_position(downloaded);
    }

    pb.finish_with_message("Download complete");

    Ok(())
}

/// Extract all files from the zip archive's build/bin/ directory.
/// This includes the main binaries (llama-server, llama-cli) and all required
/// shared libraries (.dylib on macOS, .dll on Windows, .so on Linux).
fn extract_binaries(zip_path: &Path, bin_dir: &Path) -> Result<()> {
    println!("Extracting binaries and libraries...");

    let file = File::open(zip_path).context("Failed to open downloaded archive")?;
    let mut archive = zip::ZipArchive::new(file).context("Failed to read zip archive")?;

    fs::create_dir_all(bin_dir).context("Failed to create bin directory")?;

    #[cfg(target_os = "windows")]
    let required_binaries = ["llama-server.exe", "llama-cli.exe"];
    #[cfg(not(target_os = "windows"))]
    let required_binaries = ["llama-server", "llama-cli"];

    let mut extracted_binaries = 0;
    let mut extracted_libs = 0;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).context("Failed to read archive entry")?;
        let entry_name = entry.name().to_string();

        // Skip directories
        if entry.is_dir() {
            continue;
        }

        // Only extract files from build/bin/ directory
        if !entry_name.contains("build/bin/") {
            continue;
        }

        // Get the filename (last component of path)
        let file_name = match entry_name.rsplit('/').next() {
            Some(name) if !name.is_empty() => name,
            _ => continue,
        };

        // Skip license files and source/header files
        if file_name.starts_with("LICENSE") 
            || file_name.ends_with(".h") 
            || file_name.ends_with(".metal")
        {
            continue;
        }

        let dest_path = bin_dir.join(file_name);
        let mut dest_file = File::create(&dest_path)
            .with_context(|| format!("Failed to create file: {}", dest_path.display()))?;

        io::copy(&mut entry, &mut dest_file)
            .with_context(|| format!("Failed to extract: {}", file_name))?;

        // Set executable permission on Unix for binaries and libraries
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&dest_path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&dest_path, perms)?;
        }

        // Track what we extracted
        if required_binaries.iter().any(|&b| file_name == b) {
            println!("  ✓ Extracted {}", file_name);
            extracted_binaries += 1;
        } else {
            extracted_libs += 1;
        }
    }

    if extracted_binaries != required_binaries.len() {
        bail!(
            "Failed to extract all required binaries. Found {} of {}",
            extracted_binaries,
            required_binaries.len()
        );
    }

    println!("  ✓ Extracted {} shared libraries", extracted_libs);

    Ok(())
}

/// Download and install pre-built llama.cpp binaries.
///
/// This is the main entry point for downloading pre-built binaries.
/// It fetches the latest release from GitHub, downloads the appropriate
/// platform-specific archive, and extracts the binaries.
///
/// Returns `Ok(())` on success, or an error if download/extraction fails.
pub async fn download_prebuilt_binaries() -> Result<()> {
    // Check platform availability
    let availability = check_prebuilt_availability();
    let (asset_pattern, description) = match availability {
        PrebuiltAvailability::Available {
            asset_pattern,
            description,
        } => (asset_pattern, description),
        PrebuiltAvailability::NotAvailable { reason } => {
            bail!("Pre-built binaries not available: {}", reason);
        }
    };

    println!();
    println!("Downloading pre-built llama.cpp binaries for {}...", description);
    println!();

    let client = Client::new();

    // Fetch latest release
    println!("Fetching latest release information...");
    let release = fetch_latest_release(&client).await?;
    println!("  Found release: {}", release.tag_name);

    // Find matching asset
    let asset = find_platform_asset(&release, &asset_pattern).ok_or_else(|| {
        anyhow::anyhow!(
            "No matching asset found for pattern '{}' in release {}",
            asset_pattern,
            release.tag_name
        )
    })?;

    println!("  Asset: {} ({:.1} MB)", asset.name, asset.size as f64 / 1_000_000.0);
    println!();

    // Prepare paths
    let gglib_dir = get_gglib_data_dir()?;
    let download_dir = gglib_dir.join("downloads");
    let zip_path = download_dir.join(&asset.name);
    let bin_dir = gglib_dir.join("bin");

    // Download the archive
    download_with_progress(&client, &asset.browser_download_url, &zip_path).await?;
    println!();

    // Extract binaries
    extract_binaries(&zip_path, &bin_dir)?;

    // Clean up downloaded archive
    let _ = fs::remove_file(&zip_path);
    let _ = fs::remove_dir(&download_dir);

    // Save a simple config indicating this was a pre-built install
    save_prebuilt_config(&gglib_dir, &release.tag_name, &description)?;

    // Verify installation
    let server_path = get_llama_server_path()?;
    let cli_path = get_llama_cli_path()?;

    if !server_path.exists() || !cli_path.exists() {
        bail!("Installation verification failed: binaries not found after extraction");
    }

    println!();
    println!("✓ llama.cpp installed successfully!");
    println!("  Server: {}", server_path.display());
    println!("  CLI: {}", cli_path.display());
    println!("  Version: {}", release.tag_name);
    println!("  Type: Pre-built ({})", description);
    println!();
    println!("You can now use 'gglib serve', 'gglib proxy', and 'gglib chat'.");

    Ok(())
}

/// Save configuration for pre-built installation.
fn save_prebuilt_config(gglib_dir: &Path, version: &str, platform: &str) -> Result<()> {
    use serde::Serialize;

    #[derive(Serialize)]
    struct PrebuiltConfig {
        version: String,
        platform: String,
        install_type: String,
        installed_at: String,
    }

    let config = PrebuiltConfig {
        version: version.to_string(),
        platform: platform.to_string(),
        install_type: "prebuilt".to_string(),
        installed_at: chrono::Utc::now().to_rfc3339(),
    };

    let config_path = gglib_dir.join("llama-config.json");
    let json = serde_json::to_string_pretty(&config)?;
    fs::write(&config_path, json)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_prebuilt_availability() {
        let availability = check_prebuilt_availability();
        // Just verify it doesn't panic and returns a valid variant
        match availability {
            PrebuiltAvailability::Available { .. } => {}
            PrebuiltAvailability::NotAvailable { .. } => {}
        }
    }
}
