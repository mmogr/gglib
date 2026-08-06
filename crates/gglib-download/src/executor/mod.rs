#![doc = include_str!("README.md")]
mod native;

use std::path::Path;
use std::sync::{Arc, OnceLock};

use reqwest::Client;
use tokio_util::sync::CancellationToken;

use gglib_core::download::DownloadError;

use crate::cli_exec::{
    FastDownloadRequest, NoticeCallback, ProgressCallback, PythonBridgeError,
    fast_helper_provisioned, run_fast_download,
};

use native::{NativeError, existing_len};

/// Shared HTTP client. `reqwest::Client` owns a connection pool, so building one
/// per file would throw away connection reuse across a sharded model.
///
/// Configuration lives in [`native::build_client`] — see it for why automatic
/// redirect following must stay off.
fn http_client() -> &'static Client {
    static CLIENT: OnceLock<Client> = OnceLock::new();
    CLIENT.get_or_init(native::build_client)
}

/// A set of files to fetch into one directory.
pub struct DownloadPlan<'a> {
    /// `owner/name` on `HuggingFace`.
    pub repo_id: &'a str,
    /// Branch, tag, or commit SHA.
    pub revision: &'a str,
    /// Directory the files land in.
    pub destination: &'a Path,
    /// Paths within the repository, relative to its root.
    pub files: &'a [String],
    /// Bearer token for private repositories.
    pub token: Option<&'a str>,
    /// Re-fetch even if the file is already on disk.
    pub force: bool,
    /// Sink for `(downloaded, total)` byte counts, aggregated across `files`.
    pub progress: Option<ProgressCallback>,
    /// Sink for transient notes that carry no byte progress.
    pub notice: Option<NoticeCallback>,
    /// Total size from `HuggingFace` metadata, when known.
    pub expected_total: Option<u64>,
    /// Cancellation token for the whole plan.
    pub cancel: Option<CancellationToken>,
}

/// Fetch every file in `plan`.
///
/// The native Rust path is the default. The `hf_xet` accelerator is used only
/// when its environment is **already** provisioned on this machine — it is never
/// built implicitly, because that put a Python toolchain in the way of a new
/// user's first download. If the accelerator is present but fails, this falls
/// back to the native path rather than failing the download.
pub async fn download_files(plan: &DownloadPlan<'_>) -> Result<(), DownloadError> {
    if plan.files.is_empty() {
        return Ok(());
    }

    if fast_helper_provisioned() {
        match run_accelerated(plan).await {
            Ok(()) => return Ok(()),
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
            }
        }
    } else {
        suggest_accelerator_once(plan);
    }

    run_native(plan).await
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
async fn run_accelerated(plan: &DownloadPlan<'_>) -> Result<(), PythonBridgeError> {
    let request = FastDownloadRequest {
        repo_id: plan.repo_id,
        revision: plan.revision,
        repo_type: "model",
        destination: plan.destination,
        files: plan.files,
        token: plan.token,
        force: plan.force,
        progress: plan.progress.clone(),
        notice: plan.notice.clone(),
        expected_total: plan.expected_total,
        cancel_token: plan.cancel.clone(),
    };

    run_fast_download(&request).await
}

/// Fetch each file over plain HTTPS.
async fn run_native(plan: &DownloadPlan<'_>) -> Result<(), DownloadError> {
    // Progress is reported across the whole plan, not per file, so a sharded
    // model does not reset its bar to zero on every shard.
    let mut completed_bytes: u64 = 0;

    for file in plan.files {
        let dest = plan.destination.join(file);

        if plan.force {
            let _ = std::fs::remove_file(&dest);
        } else if dest.exists() {
            // Already here. The manager removes files that fail validation
            // before we are called, so anything still present is trusted.
            completed_bytes += existing_len(&dest);
            continue;
        }

        let url = gglib_hf::build_file_url(plan.repo_id, file, Some(plan.revision));

        // Only a single-file plan can attribute `expected_total` to one file;
        // for a multi-file plan the per-file size is not known here.
        let expected_size = (plan.files.len() == 1)
            .then_some(plan.expected_total)
            .flatten();

        let request = native::NativeDownload {
            url: &url,
            dest: &dest,
            token: plan.token,
            expected_size,
            progress: offset_progress(plan.progress.as_ref(), completed_bytes),
            cancel: plan.cancel.clone(),
        };

        native::download_file(http_client(), &request)
            .await
            .map_err(to_download_error)?;

        completed_bytes += existing_len(&dest);
    }

    Ok(())
}

/// Shift a per-file progress callback by the bytes already finished, so the
/// aggregate reported to the caller only ever moves forward.
fn offset_progress(progress: Option<&ProgressCallback>, offset: u64) -> Option<ProgressCallback> {
    let inner = Arc::clone(progress?);
    if offset == 0 {
        return Some(inner);
    }
    Some(Arc::new(move |downloaded, total| {
        inner(downloaded + offset, total + offset);
    }))
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
mod tests {
    use super::*;

    #[test]
    fn offset_progress_shifts_both_counts() {
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        let inner: ProgressCallback = Arc::new(move |d, t| sink.lock().unwrap().push((d, t)));

        let shifted = offset_progress(Some(&inner), 100).expect("callback present");
        shifted(50, 400);

        assert_eq!(*seen.lock().unwrap(), vec![(150, 500)]);
    }

    #[test]
    fn offset_progress_passes_through_at_zero() {
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        let inner: ProgressCallback = Arc::new(move |d, t| sink.lock().unwrap().push((d, t)));

        let shifted = offset_progress(Some(&inner), 0).expect("callback present");
        shifted(50, 400);

        assert_eq!(*seen.lock().unwrap(), vec![(50, 400)]);
    }

    #[test]
    fn offset_progress_is_none_without_a_sink() {
        assert!(offset_progress(None, 10).is_none());
    }

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
