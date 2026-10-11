//! The pre-built install pipeline, from the release listing to the verified
//! binary.

use anyhow::{Result, bail};
use reqwest::Client;
use std::fs;
use std::path::Path;
use tokio::sync::mpsc;

use super::extract::extract_binaries;
use super::fetch::{download_archive, download_cuda_runtime, fetch_release};
use super::record::PrebuiltRecord;
use super::release::{AssetMatcher, GITHUB_API, ReleaseSelector, ReleaseSpec, resolve_selector};
use crate::llama::{InstallPhase, LlamaProgressEvent};

/// What one platform installs from a release.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PrebuiltTarget<'a> {
    /// Which asset is this platform's.
    pub(crate) matcher: AssetMatcher<'a>,
    /// The platform build as the record names it (`macOS ARM64 (Metal)`).
    pub(crate) description: &'a str,
    /// The archive members that must be there, by file name.
    pub(crate) required: &'a [&'a str],
    /// Where the files are unpacked.
    pub(crate) bin_dir: &'a Path,
    /// The binary the launcher runs, checked once everything is unpacked.
    pub(crate) server_path: &'a Path,
    /// For a CUDA build, a name the release's CUDA runtime package holds.
    pub(crate) cuda_runtime: Option<&'a str>,
}

/// Emit `PhaseStarted` for `phase`, ignoring a receiver that has gone away.
///
/// A dropped receiver means the surface stopped watching — a cancelled CLI, a
/// closed SSE connection. That is not a reason to abandon an install that is
/// already writing to disk.
pub(crate) async fn started(tx: &mpsc::Sender<LlamaProgressEvent>, phase: InstallPhase) {
    let _ = tx.send(LlamaProgressEvent::PhaseStarted { phase }).await;
}

/// Emit `PhaseCompleted` for `phase`. See [`started`].
pub(crate) async fn completed(tx: &mpsc::Sender<LlamaProgressEvent>, phase: InstallPhase) {
    let _ = tx.send(LlamaProgressEvent::PhaseCompleted { phase }).await;
}

/// Download and install `target` from `spec`'s release, streaming progress.
///
/// Resolves the release `spec`'s pin or override names, downloads this
/// platform's archive, extracts the binary and its shared libraries, hands
/// the record to `save_record` and verifies the binary landed where the
/// launcher looks for it. Every stage from [`InstallPhase::FetchRelease`] on
/// is bracketed by [`LlamaProgressEvent::PhaseStarted`] and
/// [`LlamaProgressEvent::PhaseCompleted`] on `tx`; the download itself also
/// reports bytes, rate and ETA. Deciding the target is the caller's
/// [`InstallPhase::CheckAvailability`].
///
/// Success ends with [`LlamaProgressEvent::Completed`]. Failure returns `Err`
/// *without* emitting [`LlamaProgressEvent::Failed`] — the surface that owns
/// the channel decides how a failure is worded, exactly as the source-build
/// pipeline leaves it.
pub(crate) async fn install_prebuilt(
    spec: &ReleaseSpec,
    target: PrebuiltTarget<'_>,
    save_record: impl FnOnce(PrebuiltRecord) -> Result<()>,
    tx: &mpsc::Sender<LlamaProgressEvent>,
) -> Result<()> {
    install_prebuilt_from(
        GITHUB_API,
        &resolve_selector(spec),
        spec,
        target,
        save_record,
        tx,
    )
    .await
}

/// [`install_prebuilt`] against the API at `api_base`, for `selector`.
pub(crate) async fn install_prebuilt_from(
    api_base: &str,
    selector: &ReleaseSelector,
    spec: &ReleaseSpec,
    target: PrebuiltTarget<'_>,
    save_record: impl FnOnce(PrebuiltRecord) -> Result<()>,
    tx: &mpsc::Sender<LlamaProgressEvent>,
) -> Result<()> {
    let client = Client::new();

    started(tx, InstallPhase::FetchRelease).await;
    let release = fetch_release(&client, api_base, spec, selector).await?;
    let asset = target.matcher.pick(&release, spec.choice)?;
    completed(tx, InstallPhase::FetchRelease).await;

    // The binaries go where the launcher runs them from, and the record
    // where every reader looks for it. Only the archive is this install's
    // own, to delete once it is unpacked.
    let download_dir = (spec.download_dir)()?;
    let archive_path = download_dir.join(&asset.name);

    started(tx, InstallPhase::Download).await;
    download_archive(&client, &asset.browser_download_url, &archive_path, tx).await?;
    completed(tx, InstallPhase::Download).await;

    // Capture the result so the downloads dir is cleaned up on both the
    // success and the failure path.
    let post_download_result = async {
        started(tx, InstallPhase::Extract).await;
        extract_binaries(spec, target.required, &archive_path, target.bin_dir)?;
        completed(tx, InstallPhase::Extract).await;

        // A CUDA build also needs the CUDA runtime DLLs; Vulkan builds
        // bundle everything they need inside the main archive.
        if let Some(pattern) = target.cuda_runtime {
            started(tx, InstallPhase::CudaRuntime).await;
            download_cuda_runtime(&client, &release, pattern, target.bin_dir, &download_dir)
                .await?;
            completed(tx, InstallPhase::CudaRuntime).await;
        }

        Ok::<_, anyhow::Error>(())
    }
    .await;

    // Always remove the entire downloads directory regardless of outcome.
    // Using remove_dir_all so a partially-downloaded or leftover CUDA archive
    // doesn't prevent the directory from being deleted.
    let _ = fs::remove_dir_all(&download_dir);

    post_download_result?;

    save_record(PrebuiltRecord::new(&release.tag_name, target.description))?;

    started(tx, InstallPhase::Verify).await;
    if !target.server_path.exists() {
        bail!("Installation verification failed: binaries not found after extraction");
    }
    completed(tx, InstallPhase::Verify).await;

    let _ = tx
        .send(LlamaProgressEvent::Completed {
            version: release.tag_name,
        })
        .await;

    Ok(())
}
