#![doc = include_str!("README.md")]
mod native;
mod progress;

use std::future::Future;
use std::path::Path;
use std::sync::{Arc, OnceLock};

use reqwest::Client;
use tokio_util::sync::CancellationToken;

use gglib_core::download::DownloadError;

use crate::cli_exec::{
    FastDownloadRequest, NoticeCallback, PythonBridgeError, fast_helper_provisioned,
    run_fast_download,
};

use native::{NativeError, existing_len};

pub(crate) use progress::{FileCounter, known_size};
pub use progress::{FileProgress, ProgressCallback, RawCallback, RawProgress};

/// Shared HTTP client. `reqwest::Client` owns a connection pool, so building one
/// per file would throw away connection reuse across a sharded model.
///
/// Configuration lives in [`native::build_client`] — see it for why automatic
/// redirect following must stay off.
fn http_client() -> &'static Client {
    static CLIENT: OnceLock<Client> = OnceLock::new();
    CLIENT.get_or_init(native::build_client)
}

/// One file to fetch into a directory.
pub(crate) struct DownloadPlan<'a> {
    /// `owner/name` on `HuggingFace`.
    pub repo_id: &'a str,
    /// Branch, tag, or commit SHA.
    pub revision: &'a str,
    /// Directory the file lands in.
    pub destination: &'a Path,
    /// Path within the repository, relative to its root.
    pub file: &'a str,
    /// Bearer token for private repositories.
    pub token: Option<&'a str>,
    /// Re-fetch even if the file is already on disk.
    pub force: bool,
    /// Sink for the file's progress.
    pub progress: Option<ProgressCallback>,
    /// Sink for transient notes that carry no byte progress.
    pub notice: Option<NoticeCallback>,
    /// The file's size from `HuggingFace` metadata, when known.
    pub expected_size: Option<u64>,
    /// Cancellation token for the download.
    pub cancel: Option<CancellationToken>,
}

/// Fetch the file `plan` names.
///
/// The native Rust path is the default. The `hf_xet` accelerator is used only
/// when its environment is **already** provisioned on this machine — it is never
/// built implicitly, because that put a Python toolchain in the way of a new
/// user's first download. If the accelerator is present but fails, this falls
/// back to the native path rather than failing the download.
///
/// Both transports report to one [`FileCounter`], so the caller sees one
/// count for the file whichever of them moved its bytes, and sees it reach
/// the file's size only here, once the file is in place.
pub(crate) async fn download_file(plan: &DownloadPlan<'_>) -> Result<(), DownloadError> {
    let url = gglib_hf::build_file_url(plan.repo_id, plan.file, Some(plan.revision));
    let accelerator =
        fast_helper_provisioned().then_some(|progress| run_accelerated(plan, progress));
    fetch(plan, accelerator, &url).await
}

/// [`download_file`], given the accelerator to try first, when there is one,
/// and the address the native transport fetches from.
async fn fetch<A, F>(
    plan: &DownloadPlan<'_>,
    accelerator: Option<A>,
    url: &str,
) -> Result<(), DownloadError>
where
    A: FnOnce(RawCallback) -> F,
    F: Future<Output = Result<(), PythonBridgeError>>,
{
    let dest = plan.destination.join(plan.file);
    let counter = Arc::new(FileCounter::new(plan.expected_size, plan.progress.clone()));

    if let Some(accelerate) = accelerator {
        match accelerate(counter.raw_callback()).await {
            Ok(()) => {
                counter.finish(existing_len(&dest));
                return Ok(());
            }
            // A cancelled download is the user's decision, not an accelerator
            // failure — retrying it natively would be the opposite of what they
            // asked for.
            Err(PythonBridgeError::Cancelled) => return Err(DownloadError::Cancelled),
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "hf_xet accelerator failed, falling back to the native download path"
                );
                notify(
                    plan,
                    "accelerated download unavailable, using direct transfer…",
                );
                // The native path keeps its own partial file, so the count
                // of bytes on disk may start again. The notice says why.
                counter.restart();
            }
        }
    } else {
        suggest_accelerator_once(plan);
    }

    run_native(plan, &dest, url, &counter).await?;
    counter.finish(existing_len(&dest));
    Ok(())
}

/// One-time (per process) hint that the parallel accelerator exists.
///
/// Once per process, not per plan: a sharded model runs one plan per shard,
/// and repeating the hint on every shard would read as nagging.
fn suggest_accelerator_once(plan: &DownloadPlan<'_>) {
    use std::sync::atomic::{AtomicBool, Ordering};
    static SUGGESTED: AtomicBool = AtomicBool::new(false);
    if SUGGESTED.swap(true, Ordering::Relaxed) {
        return;
    }

    let hint = "using the built-in downloader — `gglib config fast-downloads \
                enable` turns on the parallel accelerator";
    tracing::info!("{hint}");
    notify(plan, hint);
}

/// Drive the pre-existing `hf_xet` helper.
async fn run_accelerated(
    plan: &DownloadPlan<'_>,
    progress: RawCallback,
) -> Result<(), PythonBridgeError> {
    let request = FastDownloadRequest {
        repo_id: plan.repo_id,
        revision: plan.revision,
        repo_type: "model",
        destination: plan.destination,
        file: plan.file,
        token: plan.token,
        force: plan.force,
        progress: Some(progress),
        notice: plan.notice.clone(),
        cancel_token: plan.cancel.clone(),
    };

    run_fast_download(&request).await
}

/// Fetch the file from `url` over plain HTTPS.
async fn run_native(
    plan: &DownloadPlan<'_>,
    dest: &Path,
    url: &str,
    counter: &Arc<FileCounter>,
) -> Result<(), DownloadError> {
    if plan.force {
        let _ = std::fs::remove_file(dest);
    } else if dest.exists() {
        // Already here. The manager removes files that fail validation
        // before we are called, so anything still present is trusted.
        return Ok(());
    }

    let request = native::NativeDownload {
        url,
        dest,
        token: plan.token,
        expected_size: known_size(plan.expected_size),
        progress: Some(counter.raw_callback()),
        cancel: plan.cancel.clone(),
    };

    native::download_file(http_client(), &request)
        .await
        .map_err(to_download_error)
}

fn notify(plan: &DownloadPlan<'_>, message: &str) {
    if let Some(notice) = plan.notice.as_ref() {
        notice(message);
    }
}

/// Map the native path's errors onto the domain error the manager already
/// understands, preserving the distinctions the surfaces render differently.
fn to_download_error(e: NativeError) -> DownloadError {
    match e {
        NativeError::NotFound(what) => DownloadError::not_found(what),
        NativeError::Http { status, message } => {
            DownloadError::network_with_status(message, status)
        }
        NativeError::Network(message) => DownloadError::network(message),
        NativeError::ChecksumMismatch { expected, actual } => {
            DownloadError::integrity_failed(expected, actual)
        }
        NativeError::SizeMismatch { expected, actual } => {
            DownloadError::integrity_failed(format!("{expected} bytes"), format!("{actual} bytes"))
        }
        NativeError::Io { operation, message } => DownloadError::io(operation, message),
        NativeError::TooManyRedirects(url) => {
            DownloadError::network(format!("too many redirects for {url}"))
        }
        NativeError::Cancelled => DownloadError::Cancelled,
    }
}

#[cfg(test)]
#[path = "native_progress_tests.rs"]
mod native_progress_tests;

#[cfg(test)]
#[path = "fetch_tests.rs"]
mod fetch_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_found_maps_to_the_domain_not_found() {
        let mapped = to_download_error(NativeError::NotFound("u/r/f.gguf".into()));
        assert!(matches!(mapped, DownloadError::NotFound { .. }));
    }

    #[test]
    fn http_status_is_preserved_for_the_surfaces() {
        let mapped = to_download_error(NativeError::Http {
            status: 503,
            message: "unavailable".into(),
        });
        match mapped {
            DownloadError::Network { status_code, .. } => assert_eq!(status_code, Some(503)),
            other => panic!("expected Network, got {other:?}"),
        }
    }

    #[test]
    fn checksum_mismatch_maps_to_integrity_failed() {
        let mapped = to_download_error(NativeError::ChecksumMismatch {
            expected: "aa".into(),
            actual: "bb".into(),
        });
        assert!(matches!(mapped, DownloadError::IntegrityFailed { .. }));
    }
}
