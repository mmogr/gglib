//! Talking to GitHub: the release listing, the archive, and the CUDA runtime
//! package a Windows CUDA build ships beside it.

use anyhow::{Context, Result, bail};
use reqwest::Client;
use serde::Deserialize;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;
use std::time::Instant;
use tokio::sync::mpsc;

use gglib_core::download::{ProgressThrottle, RateEstimator};

use super::release::{ReleaseSelector, ReleaseSpec};
use crate::llama::LlamaProgressEvent;

/// GitHub API response for a release
#[derive(Debug, Deserialize)]
pub(crate) struct GitHubRelease {
    pub(crate) tag_name: String,
    pub(crate) assets: Vec<GitHubAsset>,
}

/// GitHub API response for a release asset
#[derive(Debug, Deserialize)]
pub(crate) struct GitHubAsset {
    pub(crate) name: String,
    pub(crate) browser_download_url: String,
}

/// Fetch `spec`'s release information for `selector` from the API at
/// `api_base`.
///
/// A 404 on a tag selector is reported as a missing release rather than a
/// bare HTTP error, because the actionable cause — a pin naming a tag that
/// upstream no longer publishes — is not obvious from the status code, and
/// the way out is an environment variable the user has no reason to know
/// about.
pub(crate) async fn fetch_release(
    client: &Client,
    api_base: &str,
    spec: &ReleaseSpec,
    selector: &ReleaseSelector,
) -> Result<GitHubRelease> {
    let response = client
        .get(selector.api_url(api_base, spec.repo))
        .header("User-Agent", "gglib")
        .header("Accept", "application/vnd.github.v3+json")
        .send()
        .await
        .with_context(|| format!("Failed to fetch {} releases from GitHub", spec.product))?;

    if response.status() == reqwest::StatusCode::NOT_FOUND
        && let ReleaseSelector::Tag(tag) = selector
    {
        let (product, env) = (spec.product, spec.env);
        bail!(
            "{product} release '{tag}' not found upstream. \
             Set {env}=latest to install the current release, \
             or {env}=<tag> to name a different one."
        );
    }

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

/// Stream `url` to `dest`, reporting bytes, rate and ETA on `tx`.
///
/// Throughput is measured here and only here, by the same [`RateEstimator`]
/// the model-download path uses. The estimator sees every chunk — ticks where
/// nothing moved are how a stall pulls the reported rate down — while its
/// sibling [`ProgressThrottle`] rate-limits the *emission*, so a fast link
/// cannot flood a 64-slot channel.
pub(crate) async fn download_archive(
    client: &Client,
    url: &str,
    dest: &Path,
    tx: &mpsc::Sender<LlamaProgressEvent>,
) -> Result<()> {
    use futures_util::StreamExt;

    let response = client
        .get(url)
        .header("User-Agent", "gglib")
        .send()
        .await
        .context("Failed to start download")?;

    if !response.status().is_success() {
        bail!("Download failed: HTTP {}", response.status());
    }

    let total = response.content_length().unwrap_or(0);

    // Ensure parent directory exists
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).context("Failed to create download directory")?;
    }

    let mut file = File::create(dest).context("Failed to create download file")?;

    let mut estimator = RateEstimator::new(Instant::now());
    let mut throttle = ProgressThrottle::default();
    let mut downloaded: u64 = 0;
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("Error reading download stream")?;
        file.write_all(&chunk)
            .context("Error writing to download file")?;
        downloaded += chunk.len() as u64;

        estimator.record(downloaded, total, Instant::now());

        if throttle.should_emit() {
            let _ = tx.try_send(LlamaProgressEvent::Progress {
                downloaded,
                total,
                rate_bps: estimator.rate_bps(),
                eta_seconds: estimator.eta_seconds(),
            });
        }
    }

    // The throttle will usually have swallowed the last chunk, and the final
    // byte count is the one a progress bar has to land on.
    let _ = tx
        .send(LlamaProgressEvent::Progress {
            downloaded,
            total,
            rate_bps: estimator.rate_bps(),
            eta_seconds: estimator.eta_seconds(),
        })
        .await;

    Ok(())
}

/// Download the release's CUDA runtime package, the first asset whose name
/// holds `pattern`, and unpack its DLLs into `bin_dir`.
///
/// A CUDA build needs these on a machine with no CUDA toolkit installed. A
/// missing package, or one that will not download, is not fatal: the user may
/// have CUDA installed.
pub(crate) async fn download_cuda_runtime(
    client: &Client,
    release: &GitHubRelease,
    pattern: &str,
    bin_dir: &Path,
    download_dir: &Path,
) -> Result<()> {
    let Some(cudart_asset) = release
        .assets
        .iter()
        .find(|asset| asset.name.contains(pattern))
    else {
        return Ok(());
    };

    let cudart_zip_path = download_dir.join(&cudart_asset.name);

    // Download silently (no progress bar for this smaller download)
    let response = client
        .get(&cudart_asset.browser_download_url)
        .header("User-Agent", "gglib")
        .send()
        .await
        .context("Failed to download CUDA runtime")?;

    if !response.status().is_success() {
        return Ok(());
    }

    let bytes = response.bytes().await?;
    fs::write(&cudart_zip_path, &bytes)?;

    let file = File::open(&cudart_zip_path)?;
    let mut archive = zip::ZipArchive::new(file)?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let entry_name = entry.name().to_string();

        if entry.is_dir() {
            continue;
        }

        #[allow(
            clippy::case_sensitive_file_extension_comparisons,
            reason = "the packages name their DLLs in lower case; the rule llama.cpp's install always had"
        )]
        let is_dll = entry_name.ends_with(".dll");
        if is_dll {
            let file_name = entry_name.rsplit('/').next().unwrap_or(&entry_name);
            let dest_path = bin_dir.join(file_name);
            let mut dest_file = File::create(&dest_path)?;
            io::copy(&mut entry, &mut dest_file)?;
        }
    }

    let _ = fs::remove_file(&cudart_zip_path);

    Ok(())
}
