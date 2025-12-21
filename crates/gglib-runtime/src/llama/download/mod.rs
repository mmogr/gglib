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

#[cfg(feature = "prebuilt")]
use anyhow::{Context, Result, bail};
#[cfg(feature = "cli")]
use indicatif::{ProgressBar, ProgressStyle};
#[cfg(feature = "prebuilt")]
use reqwest::Client;
#[cfg(feature = "prebuilt")]
use serde::Deserialize;
#[cfg(feature = "prebuilt")]
use std::fs::{self, File};
#[cfg(feature = "prebuilt")]
use std::io::{self, Write};
#[cfg(feature = "prebuilt")]
use std::path::Path;

#[cfg(feature = "prebuilt")]
use gglib_core::paths::data_root;
use gglib_core::paths::{llama_cli_path, llama_server_path};

// Helper to convert PathError to anyhow::Error
#[cfg(feature = "prebuilt")]
fn path_err<T>(r: Result<T, gglib_core::paths::PathError>) -> Result<T> {
    r.map_err(|e| anyhow::anyhow!("{}", e))
}

/// Progress callback type for llama.cpp downloads.
/// Called with (`downloaded_bytes`, `total_bytes`).
#[cfg(feature = "prebuilt")]
pub type LlamaProgressCallback<'a> = &'a dyn Fn(u64, u64);

/// Thread-safe progress callback for async contexts.
#[cfg(feature = "prebuilt")]
pub type LlamaProgressCallbackBoxed = Box<dyn Fn(u64, u64) + Send + Sync>;

/// Check if llama.cpp binaries are installed.
/// Returns true if both llama-server and llama-cli exist.
pub fn check_llama_installed() -> bool {
    let server_path = match llama_server_path() {
        Ok(p) => p,
        Err(_) => return false,
    };
    let cli_path = match llama_cli_path() {
        Ok(p) => p,
        Err(_) => return false,
    };
    server_path.exists() && cli_path.exists()
}

/// GitHub API response for a release
#[cfg(feature = "prebuilt")]
#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    assets: Vec<GitHubAsset>,
}

/// GitHub API response for a release asset
#[cfg(feature = "prebuilt")]
#[derive(Debug, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

/// Result of checking pre-built binary availability
#[cfg(feature = "prebuilt")]
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
#[cfg(feature = "prebuilt")]
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
                asset_pattern: "bin-win-cuda-12.4-x64.zip".to_string(),
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
#[cfg(feature = "prebuilt")]
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
#[cfg(feature = "prebuilt")]
fn find_platform_asset<'a>(
    release: &'a GitHubRelease,
    asset_pattern: &str,
) -> Option<&'a GitHubAsset> {
    release
        .assets
        .iter()
        .find(|asset| asset.name.contains(asset_pattern))
}

/// Download a file with progress bar (CLI version).
#[cfg(feature = "prebuilt")]
async fn download_with_progress(client: &Client, url: &str, dest: &Path) -> Result<()> {
    download_with_callback_internal(client, url, dest, None).await
}

/// Download a file with progress callback (GUI version).
#[cfg(feature = "prebuilt")]
async fn download_with_callback(
    client: &Client,
    url: &str,
    dest: &Path,
    callback: LlamaProgressCallback<'_>,
) -> Result<()> {
    download_with_callback_internal(client, url, dest, Some(callback)).await
}

/// Internal download implementation supporting both CLI progress bar and GUI callback.
#[cfg(feature = "prebuilt")]
async fn download_with_callback_internal(
    client: &Client,
    url: &str,
    dest: &Path,
    callback: Option<LlamaProgressCallback<'_>>,
) -> Result<()> {
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

    // Use progress bar only when no callback provided AND cli feature enabled (CLI mode)
    #[cfg(feature = "cli")]
    let pb = if callback.is_none() {
        let pb = ProgressBar::new(total_size);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec})")
                .unwrap()
                .progress_chars("█▓░"),
        );
        Some(pb)
    } else {
        None
    };

    #[cfg(not(feature = "cli"))]
    let _pb: Option<()> = None;

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

        #[cfg(feature = "cli")]
        if let Some(ref pb) = pb {
            pb.set_position(downloaded);
        }
        if let Some(ref cb) = callback {
            cb(downloaded, total_size);
        }
    }

    #[cfg(feature = "cli")]
    if let Some(pb) = pb {
        pb.finish_with_message("Download complete");
    }

    Ok(())
}

/// Download a file with boxed progress callback (thread-safe for async contexts).
#[cfg(feature = "prebuilt")]
async fn download_with_boxed_callback(
    client: &Client,
    url: &str,
    dest: &Path,
    callback: &LlamaProgressCallbackBoxed,
) -> Result<()> {
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
        callback(downloaded, total_size);
    }

    Ok(())
}

/// Extract all files from the zip archive.
///
/// For macOS: extracts from build/bin/ directory
/// For Windows: extracts from root level (Windows packages have flat structure)
///
/// This includes the main binaries (llama-server, llama-cli) and all required
/// shared libraries (.dylib on macOS, .dll on Windows, .so on Linux).
#[cfg(feature = "prebuilt")]
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
        let mut entry = archive
            .by_index(i)
            .context("Failed to read archive entry")?;
        let entry_name = entry.name().to_string();

        // Skip directories
        if entry.is_dir() {
            continue;
        }

        // Platform-specific path filtering:
        // - macOS packages have binaries in build/bin/
        // - Windows packages have binaries at root level
        #[cfg(not(target_os = "windows"))]
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
        if required_binaries.contains(&file_name) {
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

/// Windows-only: Download and extract CUDA runtime DLLs.
/// These are required for llama.cpp CUDA builds to work on systems without CUDA installed.
#[cfg(all(target_os = "windows", feature = "prebuilt"))]
async fn download_cuda_runtime(
    client: &Client,
    release: &GitHubRelease,
    bin_dir: &Path,
    download_dir: &Path,
) -> Result<()> {
    const CUDART_PATTERN: &str = "cudart-llama-bin-win-cuda";

    // Find the CUDA runtime asset
    let cudart_asset = release
        .assets
        .iter()
        .find(|asset| asset.name.contains(CUDART_PATTERN));

    let cudart_asset = match cudart_asset {
        Some(asset) => asset,
        None => {
            // Not a fatal error - user might have CUDA installed
            println!("  ⚠ CUDA runtime package not found (optional if CUDA is installed)");
            return Ok(());
        }
    };

    println!(
        "  Downloading CUDA runtime DLLs ({:.1} MB)...",
        cudart_asset.size as f64 / 1_000_000.0
    );

    let cudart_zip_path = download_dir.join(&cudart_asset.name);

    // Download silently (no progress bar for this smaller download)
    let response = client
        .get(&cudart_asset.browser_download_url)
        .header("User-Agent", "gglib")
        .send()
        .await
        .context("Failed to download CUDA runtime")?;

    if !response.status().is_success() {
        println!("  ⚠ Failed to download CUDA runtime (optional if CUDA is installed)");
        return Ok(());
    }

    let bytes = response.bytes().await?;
    fs::write(&cudart_zip_path, &bytes)?;

    // Extract CUDA DLLs
    let file = File::open(&cudart_zip_path)?;
    let mut archive = zip::ZipArchive::new(file)?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let entry_name = entry.name().to_string();

        if entry.is_dir() {
            continue;
        }

        // Extract DLL files
        if entry_name.ends_with(".dll") {
            let file_name = entry_name.rsplit('/').next().unwrap_or(&entry_name);
            let dest_path = bin_dir.join(file_name);
            let mut dest_file = File::create(&dest_path)?;
            io::copy(&mut entry, &mut dest_file)?;
            println!("  ✓ Extracted {}", file_name);
        }
    }

    // Clean up
    let _ = fs::remove_file(&cudart_zip_path);

    Ok(())
}

/// Download and install pre-built llama.cpp binaries.
///
/// This is the main entry point for downloading pre-built binaries.
/// It fetches the latest release from GitHub, downloads the appropriate
/// platform-specific archive, and extracts the binaries.
///
/// Returns `Ok(())` on success, or an error if download/extraction fails.
#[cfg(feature = "prebuilt")]
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
    println!(
        "Downloading pre-built llama.cpp binaries for {}...",
        description
    );
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

    println!(
        "  Asset: {} ({:.1} MB)",
        asset.name,
        asset.size as f64 / 1_000_000.0
    );
    println!();

    // Prepare paths
    let gglib_dir = path_err(data_root())?;
    let download_dir = gglib_dir.join("downloads");
    let zip_path = download_dir.join(&asset.name);
    let bin_dir = gglib_dir.join("bin");

    // Download the archive
    download_with_progress(&client, &asset.browser_download_url, &zip_path).await?;
    println!();

    // Extract binaries
    extract_binaries(&zip_path, &bin_dir)?;

    // Windows: Also download CUDA runtime DLLs
    #[cfg(target_os = "windows")]
    download_cuda_runtime(&client, &release, &bin_dir, &download_dir).await?;

    // Clean up downloaded archive
    let _ = fs::remove_file(&zip_path);
    let _ = fs::remove_dir(&download_dir);

    // Save a simple config indicating this was a pre-built install
    save_prebuilt_config(&gglib_dir, &release.tag_name, &description)?;

    // Verify installation
    let server_path = path_err(llama_server_path())?;
    let cli_path = path_err(llama_cli_path())?;

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

/// Download and install pre-built llama.cpp binaries with progress callback.
///
/// This is the GUI-friendly version that accepts a progress callback instead
/// of printing to stdout. Used by Tauri GUI for showing download progress.
///
/// The callback receives (`downloaded_bytes`, `total_bytes`).
#[cfg(feature = "prebuilt")]
pub async fn download_prebuilt_binaries_with_callback(
    progress_callback: Option<LlamaProgressCallback<'_>>,
) -> Result<()> {
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

    let client = Client::new();

    // Fetch latest release
    let release = fetch_latest_release(&client).await?;

    // Find matching asset
    let asset = find_platform_asset(&release, &asset_pattern).ok_or_else(|| {
        anyhow::anyhow!(
            "No matching asset found for pattern '{}' in release {}",
            asset_pattern,
            release.tag_name
        )
    })?;

    // Prepare paths
    let gglib_dir = path_err(data_root())?;
    let download_dir = gglib_dir.join("downloads");
    let zip_path = download_dir.join(&asset.name);
    let bin_dir = gglib_dir.join("bin");

    // Download the archive
    if let Some(callback) = progress_callback {
        download_with_callback(&client, &asset.browser_download_url, &zip_path, callback).await?;
    } else {
        download_with_progress(&client, &asset.browser_download_url, &zip_path).await?;
    }

    // Extract binaries (quick operation, no progress needed)
    extract_binaries(&zip_path, &bin_dir)?;

    // Windows: Also download CUDA runtime DLLs
    #[cfg(target_os = "windows")]
    download_cuda_runtime(&client, &release, &bin_dir, &download_dir).await?;

    // Clean up downloaded archive
    let _ = fs::remove_file(&zip_path);
    let _ = fs::remove_dir(&download_dir);

    // Save a simple config indicating this was a pre-built install
    save_prebuilt_config(&gglib_dir, &release.tag_name, &description)?;

    // Verify installation
    let server_path = path_err(llama_server_path())?;
    let cli_path = path_err(llama_cli_path())?;

    if !server_path.exists() || !cli_path.exists() {
        bail!("Installation verification failed: binaries not found after extraction");
    }

    Ok(())
}

/// Download and install pre-built llama.cpp binaries with thread-safe progress callback.
///
/// This version is designed for use in async contexts where the callback needs to be
/// Send + Sync (like Tauri commands). The callback receives (`downloaded_bytes`, `total_bytes`).
#[cfg(feature = "prebuilt")]
pub async fn download_prebuilt_binaries_with_boxed_callback(
    progress_callback: LlamaProgressCallbackBoxed,
) -> Result<()> {
    // Check platform availability
    let availability = check_prebuilt_availability();
    let (asset_pattern, _description) = match availability {
        PrebuiltAvailability::Available {
            asset_pattern,
            description,
        } => (asset_pattern, description),
        PrebuiltAvailability::NotAvailable { reason } => {
            bail!("Pre-built binaries not available: {}", reason);
        }
    };

    let client = Client::new();

    // Fetch latest release
    let release = fetch_latest_release(&client).await?;

    // Find matching asset
    let asset = find_platform_asset(&release, &asset_pattern).ok_or_else(|| {
        anyhow::anyhow!(
            "No matching asset found for pattern '{}' in release {}",
            asset_pattern,
            release.tag_name
        )
    })?;

    // Prepare paths
    let gglib_dir = path_err(data_root())?;
    let download_dir = gglib_dir.join("downloads");
    let zip_path = download_dir.join(&asset.name);
    let bin_dir = gglib_dir.join("bin");

    // Download the archive with boxed callback
    download_with_boxed_callback(
        &client,
        &asset.browser_download_url,
        &zip_path,
        &progress_callback,
    )
    .await?;

    // Extract binaries (quick operation, no progress needed)
    extract_binaries(&zip_path, &bin_dir)?;

    // Windows: Also download CUDA runtime DLLs
    #[cfg(target_os = "windows")]
    download_cuda_runtime(&client, &release, &bin_dir, &download_dir).await?;

    // Clean up downloaded archive
    let _ = fs::remove_file(&zip_path);
    let _ = fs::remove_dir(&download_dir);

    // Save a simple config indicating this was a pre-built install
    save_prebuilt_config(&gglib_dir, &release.tag_name, &_description)?;

    // Verify installation
    let server_path = path_err(llama_server_path())?;
    let cli_path = path_err(llama_cli_path())?;

    if !server_path.exists() || !cli_path.exists() {
        bail!("Installation verification failed: binaries not found after extraction");
    }

    Ok(())
}

/// Save configuration for pre-built installation.
#[cfg(feature = "prebuilt")]
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
