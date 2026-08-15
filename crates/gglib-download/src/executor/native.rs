//! Native Rust HTTP downloader.
//!
//! Streams a file over HTTPS with `reqwest`, resuming an interrupted transfer
//! with a ranged GET, hashing as it goes, and renaming into place only once the
//! bytes have been verified. This is the default download path — it needs
//! nothing on the machine beyond the gglib binary itself.

use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use futures_util::StreamExt;
use reqwest::header::{AUTHORIZATION, CONTENT_RANGE, HeaderMap, LOCATION, RANGE};
use reqwest::{Client, Response, StatusCode, Url};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::cli_exec::ProgressCallback;
use crate::progress::ProgressThrottle;

/// Suffix for the in-progress file that sits beside the final destination.
const PART_SUFFIX: &str = ".part";

/// Write buffer in front of the `.part` file.
///
/// Hyper hands the body over in ~8-16 KB chunks; writing each one straight to
/// the file costs a syscall per chunk. Batching them into 4 MB writes keeps
/// the sink thread almost always idle in `blocking_recv`, waiting.
const SINK_BUF_BYTES: usize = 4 * 1024 * 1024;

/// Chunks buffered between the network loop and the sink thread.
///
/// Deep enough to ride out a disk-latency spike without stalling the socket,
/// shallow enough that cancellation never waits on more than a few MB of
/// backlog draining.
const SINK_QUEUE_CHUNKS: usize = 32;

/// The only header that carries the file's true SHA-256.
///
/// `HuggingFace` sets this on the **redirect** its `resolve/` endpoint returns,
/// alongside the `Location` pointing at the CDN. It is deliberately the only
/// digest source this module trusts: the CDN's own `ETag` is the Xet block hash
/// (the same value that appears in the CDN URL path), which is 64 hex characters
/// and so indistinguishable from a SHA-256 by shape, but is *not* the hash of
/// the file's bytes. Comparing against it fails every download.
const X_LINKED_ETAG: &str = "x-linked-etag";

/// Redirect hops to follow before giving up.
const MAX_REDIRECTS: usize = 10;

/// Errors from the native download path.
#[derive(Error, Debug)]
pub(crate) enum NativeError {
    /// The remote returned 404.
    #[error("Not found: {0}")]
    NotFound(String),

    /// The remote returned a non-success status other than 404.
    #[error("HTTP {status}: {message}")]
    Http {
        /// The status code returned.
        status: u16,
        /// Detail for the user.
        message: String,
    },

    /// The transfer itself failed (DNS, TLS, connection reset, timeout).
    #[error("Network error: {0}")]
    Network(String),

    /// The bytes on disk do not match the digest the server advertised.
    #[error("Checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch {
        /// Digest advertised by the server.
        expected: String,
        /// Digest actually computed over the received bytes.
        actual: String,
    },

    /// The completed file is not the size the catalog said it would be.
    #[error("Size mismatch: expected {expected} bytes, got {actual}")]
    SizeMismatch {
        /// Size from `HuggingFace` metadata.
        expected: u64,
        /// Size actually written to disk.
        actual: u64,
    },

    /// A filesystem operation failed.
    #[error("I/O error ({operation}): {message}")]
    Io {
        /// What was being attempted.
        operation: String,
        /// The underlying error.
        message: String,
    },

    /// The redirect chain never terminated at a real response.
    #[error("Too many redirects for {0}")]
    TooManyRedirects(String),

    /// The caller cancelled the download.
    #[error("download cancelled by user")]
    Cancelled,
}

impl NativeError {
    fn io(operation: &str, e: &std::io::Error) -> Self {
        Self::Io {
            operation: operation.to_string(),
            message: e.to_string(),
        }
    }
}

/// One file to fetch.
pub(crate) struct NativeDownload<'a> {
    /// Fully-qualified URL of the file's bytes.
    pub url: &'a str,
    /// Final path. Bytes land at `<dest>.part` until they are verified.
    pub dest: &'a Path,
    /// Bearer token for private repositories.
    pub token: Option<&'a str>,
    /// Expected size from `HuggingFace` metadata, when known.
    pub expected_size: Option<u64>,
    /// Sink for `(downloaded, total)` byte counts.
    pub progress: Option<ProgressCallback>,
    /// Cancellation token. A cancelled transfer leaves its `.part` file behind
    /// so the next attempt resumes rather than restarting.
    pub cancel: Option<CancellationToken>,
}

/// Download one file, resuming if a previous attempt left bytes behind.
///
/// On success the file exists at `req.dest` and no `.part` file remains. On a
/// verification failure the `.part` file is removed, because resuming onto
/// corrupt bytes would fail the same way forever.
pub(crate) async fn download_file(
    client: &Client,
    req: &NativeDownload<'_>,
) -> Result<(), NativeError> {
    let part_path = part_path_for(req.dest);

    if let Some(parent) = req.dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| NativeError::io("create_dir", &e))?;
    }

    let resume_from = existing_len(&part_path);
    let outcome = stream_to_part(client, req, &part_path, resume_from).await?;

    verify(&part_path, req.expected_size, outcome.digest.as_ref())?;

    std::fs::rename(&part_path, req.dest).map_err(|e| NativeError::io("rename", &e))?;

    Ok(())
}

/// What a completed transfer produced.
struct TransferOutcome {
    /// SHA-256 of the whole file, when the server advertised one to check
    /// against. `None` means there was nothing to compare with, so the hash was
    /// not worth computing.
    digest: Option<Digested>,
}

/// A computed digest alongside the value it must equal.
struct Digested {
    expected: String,
    actual: String,
}

/// Stream the response body into the `.part` file.
async fn stream_to_part(
    client: &Client,
    req: &NativeDownload<'_>,
    part_path: &Path,
    resume_from: u64,
) -> Result<TransferOutcome, NativeError> {
    let (response, expected) = send_following_redirects(client, req, resume_from).await?;

    let status = response.status();

    // The server has all of it already: nothing left to transfer, go verify.
    if status == StatusCode::RANGE_NOT_SATISFIABLE && resume_from > 0 {
        return finish_without_transfer(part_path, expected);
    }

    if status == StatusCode::NOT_FOUND {
        return Err(NativeError::NotFound(req.url.to_string()));
    }
    if !status.is_success() {
        return Err(NativeError::Http {
            status: status.as_u16(),
            message: format!("download failed for {}", req.url),
        });
    }

    // A 200 to a ranged request means the server ignored the range: the body is
    // the whole file, so anything already on disk is stale.
    let appending = resume_from > 0 && status == StatusCode::PARTIAL_CONTENT;
    let start_at = if appending { resume_from } else { 0 };

    let total = total_size(response.headers(), start_at).or(req.expected_size);

    let file = open_part(part_path, appending)?;
    let mut hasher = expected.is_some().then(Sha256::new);

    // Resuming: the bytes already on disk are part of the digest, so replay them
    // through the hasher before the new ones arrive. Only paid on a resume.
    if let Some(h) = hasher.as_mut() {
        if appending {
            seed_hasher(part_path, resume_from, h)?;
        }
    }

    // Writes and hashing happen on a dedicated blocking thread, fed through a
    // bounded channel, so network reads overlap disk I/O instead of
    // serializing with it — and no async worker thread ever blocks on a
    // syscall. Every exit path below drops `tx` and joins the sink, which
    // flushes whatever arrived; on errors and cancellation that keeps the
    // `.part` file resumable.
    let (tx, rx) = mpsc::channel::<bytes::Bytes>(SINK_QUEUE_CHUNKS);
    let mut sink = spawn_sink(file, hasher, rx);

    let mut downloaded = start_at;
    // The initial report is unconditional: downstream relies on seq moving off
    // 0 to know a transfer has started (see emit_synthetic_progress_if_cached).
    report(req.progress.as_ref(), downloaded, total);

    // Per-chunk reporting hammered the watch channel thousands of times a
    // second; consumers sample every 250ms anyway. 100ms keeps the final
    // display one tick fresh while shedding ~99% of the sends.
    let mut throttle = ProgressThrottle::default_interval();

    let mut stream = response.bytes_stream();
    loop {
        let chunk = if let Some(cancel) = req.cancel.as_ref() {
            tokio::select! {
                biased;
                () = cancel.cancelled() => {
                    drop(tx);
                    let _ = sink.await;
                    return Err(NativeError::Cancelled);
                }
                chunk = stream.next() => chunk,
            }
        } else {
            stream.next().await
        };

        let Some(chunk) = chunk else { break };
        let chunk = match chunk {
            Ok(chunk) => chunk,
            Err(e) => {
                drop(tx);
                let _ = sink.await;
                return Err(NativeError::Network(e.to_string()));
            }
        };

        let len = chunk.len() as u64;
        if tx.send(chunk).await.is_err() {
            // The sink died; its join result carries the real I/O error.
            break;
        }

        downloaded += len;
        if throttle.should_emit() {
            report(req.progress.as_ref(), downloaded, total);
        }
    }

    drop(tx);
    let hasher = join_sink(&mut sink).await?;

    // Unconditional final report so the last sample lands on the exact final
    // byte count instead of wherever the throttle last let one through.
    report(req.progress.as_ref(), downloaded, total);

    Ok(TransferOutcome {
        digest: hasher.map(|h| Digested {
            expected: expected.unwrap_or_default(),
            actual: hex(&h.finalize()),
        }),
    })
}

/// The sink half of the transfer: a blocking thread that owns the `.part`
/// file and the hasher, draining chunks the network loop sends it.
///
/// Returns the hasher once the channel closes and the buffer is flushed.
fn spawn_sink(
    file: std::fs::File,
    mut hasher: Option<Sha256>,
    mut rx: mpsc::Receiver<bytes::Bytes>,
) -> tokio::task::JoinHandle<std::io::Result<Option<Sha256>>> {
    tokio::task::spawn_blocking(move || {
        let mut writer = BufWriter::with_capacity(SINK_BUF_BYTES, file);
        while let Some(chunk) = rx.blocking_recv() {
            writer.write_all(&chunk)?;
            if let Some(h) = hasher.as_mut() {
                h.update(&chunk);
            }
        }
        writer.flush()?;
        Ok(hasher)
    })
}

/// Join the sink thread, mapping its two failure layers onto [`NativeError`].
async fn join_sink(
    sink: &mut tokio::task::JoinHandle<std::io::Result<Option<Sha256>>>,
) -> Result<Option<Sha256>, NativeError> {
    match sink.await {
        Ok(Ok(hasher)) => Ok(hasher),
        Ok(Err(e)) => Err(NativeError::io("write", &e)),
        Err(e) => Err(NativeError::Io {
            operation: "write".to_string(),
            message: format!("sink thread panicked: {e}"),
        }),
    }
}

/// Build the HTTP client this module requires.
///
/// Automatic redirect following **must** be off: [`send_following_redirects`]
/// walks the chain itself so the `X-Linked-Etag` on `HuggingFace`'s 302 survives
/// to be compared against. A client that follows redirects on its own hands this
/// module only the CDN's headers, where no trustworthy digest exists — and
/// verification silently turns into a no-op.
///
/// Tests construct their client through here too, so the two cannot drift.
pub(super) fn build_client() -> Client {
    Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        // Bound connection establishment, but no overall request timeout:
        // multi-GB bodies legitimately stream for hours.
        .connect_timeout(std::time::Duration::from_secs(30))
        // Sharded models fetch one file after another from the same hosts;
        // keeping a few connections warm skips a TLS handshake per shard.
        .pool_max_idle_per_host(4)
        .pool_idle_timeout(std::time::Duration::from_secs(90))
        .build()
        .expect("default reqwest client builds")
}

/// Send the request, following redirects by hand, and return the final response
/// together with any `X-Linked-Etag` seen along the way.
///
/// Redirects are followed manually rather than by `reqwest` because the digest
/// lives on a hop that automatic following would consume and discard:
/// `HuggingFace`'s `resolve/` endpoint answers with a 302 carrying
/// `X-Linked-Etag`, and only the `Location` it points at serves the bytes. With
/// `reqwest` following the chain itself, the only headers left are the CDN's —
/// whose `ETag` is a Xet block hash, not a content hash.
async fn send_following_redirects(
    client: &Client,
    req: &NativeDownload<'_>,
    resume_from: u64,
) -> Result<(Response, Option<String>), NativeError> {
    let mut url = Url::parse(req.url)
        .map_err(|e| NativeError::Network(format!("invalid URL {}: {e}", req.url)))?;
    let origin_host = url.host_str().map(str::to_owned);
    let mut linked_etag: Option<String> = None;

    for _ in 0..MAX_REDIRECTS {
        let mut request = client.get(url.clone()).header("User-Agent", "gglib");

        // The token authenticates us to HuggingFace, not to its CDN. The
        // redirect target is a pre-signed URL that needs no credentials, and
        // forwarding a bearer token to a different host would leak it.
        if let Some(token) = req.token {
            if url.host_str().map(str::to_owned) == origin_host {
                request = request.header(AUTHORIZATION, format!("Bearer {token}"));
            }
        }
        if resume_from > 0 {
            request = request.header(RANGE, format!("bytes={resume_from}-"));
        }

        let response = request
            .send()
            .await
            .map_err(|e| NativeError::Network(e.to_string()))?;

        // Capture the digest from the first hop that offers one — later hops
        // (the CDN) do not carry it.
        if linked_etag.is_none() {
            linked_etag = expected_digest(response.headers());
        }

        if !response.status().is_redirection() {
            return Ok((response, linked_etag));
        }

        let location = response
            .headers()
            .get(LOCATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| {
                NativeError::Network(format!("redirect from {url} carried no Location"))
            })?;

        url = url
            .join(location)
            .map_err(|e| NativeError::Network(format!("bad redirect target {location}: {e}")))?;
    }

    Err(NativeError::TooManyRedirects(req.url.to_string()))
}

/// Nothing to transfer (HTTP 416): hash whatever is on disk so verification can
/// still run against it.
fn finish_without_transfer(
    part_path: &Path,
    expected: Option<String>,
) -> Result<TransferOutcome, NativeError> {
    let Some(expected) = expected else {
        return Ok(TransferOutcome { digest: None });
    };

    let mut hasher = Sha256::new();
    let len = existing_len(part_path);
    seed_hasher(part_path, len, &mut hasher)?;

    Ok(TransferOutcome {
        digest: Some(Digested {
            expected,
            actual: hex(&hasher.finalize()),
        }),
    })
}

/// Check the finished `.part` file against everything we know about it.
///
/// A failure here removes the `.part` file: resuming onto bytes that are already
/// known-bad would fail identically on every future attempt.
fn verify(
    part_path: &Path,
    expected_size: Option<u64>,
    digest: Option<&Digested>,
) -> Result<(), NativeError> {
    if let Some(d) = digest {
        if !d.expected.eq_ignore_ascii_case(&d.actual) {
            let _ = std::fs::remove_file(part_path);
            return Err(NativeError::ChecksumMismatch {
                expected: d.expected.clone(),
                actual: d.actual.clone(),
            });
        }
    }

    // Size is a weaker check than the digest, but it is the only one available
    // when the server advertised no usable etag.
    //
    // Short of the expected size means the transfer was cut off, and those bytes
    // are a valid prefix — keep them so the next attempt resumes. Longer means
    // the file is not what the catalog described, and there is nothing to
    // salvage.
    if let Some(expected) = expected_size {
        let actual = existing_len(part_path);
        if actual != expected {
            if actual > expected {
                let _ = std::fs::remove_file(part_path);
            }
            return Err(NativeError::SizeMismatch { expected, actual });
        }
    }

    Ok(())
}

// ============================================================================
// Helpers
// ============================================================================

fn part_path_for(dest: &Path) -> PathBuf {
    let mut name = dest.as_os_str().to_os_string();
    name.push(PART_SUFFIX);
    PathBuf::from(name)
}

/// Length of a file, or 0 if it is missing or unreadable.
pub(super) fn existing_len(path: &Path) -> u64 {
    std::fs::metadata(path).map_or(0, |m| m.len())
}

fn open_part(path: &Path, appending: bool) -> Result<std::fs::File, NativeError> {
    std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(appending)
        .truncate(!appending)
        .open(path)
        .map_err(|e| NativeError::io("open", &e))
}

/// Feed the first `len` bytes of `path` through `hasher`.
///
/// Only ever runs on a resume, where the bytes already on disk are part of the
/// digest but were never streamed through the hasher. Costs one extra read of
/// the partial file, and only then.
fn seed_hasher(path: &Path, len: u64, hasher: &mut Sha256) -> Result<(), NativeError> {
    let file = std::fs::File::open(path).map_err(|e| NativeError::io("open", &e))?;
    std::io::copy(&mut file.take(len), hasher).map_err(|e| NativeError::io("read", &e))?;
    Ok(())
}

fn report(progress: Option<&ProgressCallback>, downloaded: u64, total: Option<u64>) {
    if let Some(cb) = progress {
        cb(downloaded, total.unwrap_or(0));
    }
}

/// The SHA-256 the server says the file should hash to, if it gave a usable one.
///
/// Only [`X_LINKED_ETAG`] is trusted. Plain `ETag` is deliberately **not** a
/// fallback: `HuggingFace`'s CDN sets it to the Xet block hash, which is also 64
/// hex characters, so no shape check can tell the two apart — and comparing a
/// file's contents against a block hash fails every single download. A response
/// with no `X-Linked-Etag` yields `None`, and verification falls back to the
/// size check.
fn expected_digest(headers: &HeaderMap) -> Option<String> {
    headers
        .get(X_LINKED_ETAG)
        .and_then(|v| v.to_str().ok())
        .map(normalize_etag)
        .filter(|e| is_sha256_hex(e))
}

/// Strip a weak-validator prefix and surrounding quotes.
fn normalize_etag(raw: &str) -> String {
    raw.trim()
        .trim_start_matches("W/")
        .trim_matches('"')
        .to_string()
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Total size of the whole file, from `Content-Range` if present (which is
/// authoritative on a partial response) or `Content-Length` plus what is already
/// on disk.
fn total_size(headers: &HeaderMap, start_at: u64) -> Option<u64> {
    if let Some(total) = headers
        .get(CONTENT_RANGE)
        .and_then(|v| v.to_str().ok())
        .and_then(parse_content_range_total)
    {
        return Some(total);
    }

    headers
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .map(|len| len + start_at)
}

/// Pull the total out of `bytes <start>-<end>/<total>`. A `*` total means the
/// server does not know it.
fn parse_content_range_total(value: &str) -> Option<u64> {
    value.rsplit('/').next()?.trim().parse::<u64>().ok()
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::new(), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}

#[cfg(test)]
#[path = "native_tests.rs"]
mod tests;
