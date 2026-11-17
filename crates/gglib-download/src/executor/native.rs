//! Native Rust HTTP downloader.
//!
//! Streams a file over HTTPS with `reqwest`, resuming an interrupted transfer
//! with a ranged GET, hashing as it goes, and renaming into place only once the
//! bytes have been verified. This is the default download path — it needs
//! nothing on the machine beyond the gglib binary itself.

use std::io::{BufRead, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use futures_util::StreamExt;
use reqwest::header::{AUTHORIZATION, CONTENT_RANGE, HeaderMap, LOCATION, RANGE};
use reqwest::{Client, Response, StatusCode, Url};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::progress::{RawCallback, RawProgress};
use crate::cli_exec::NoticeCallback;
use gglib_core::download::{ProgressThrottle, format_duration, format_size};

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

/// How long a connection may send nothing before it is dropped and the
/// transfer carries on over a new one, from the bytes it has.
///
/// The Hub's CDN sends a file without pause, and TCP itself recovers a lost
/// packet within a few seconds, so half a minute of nothing is a connection
/// that has gone quiet and will not come back (#2061: one stayed quiet for
/// good at 3.85 GiB while a fresh one ran at 14.5 MB/s). It is short enough
/// that a user sees the reconnect before wondering.
pub(crate) const STALL_AFTER: Duration = Duration::from_secs(30);

/// How long a connection is given to open. Shorter than [`STALL_AFTER`], so a
/// connection that will not open ends with the client's own error and is
/// never taken for a quiet one. (A refused connection ends sooner: at once
/// on Unix, after a few seconds of retried attempts on Windows.)
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);

/// Reconnects after a lost connection before the download gives up.
///
/// They count only while nothing arrives: a connection that took the part
/// further before it was lost starts the count again. Each lost connection
/// that brought nothing spans at least [`STALL_AFTER`] before the next, so a
/// download whose connection goes quiet gives up two minutes after its last
/// byte when the reconnects open and go quiet too (four quiet times, and a
/// little more when they are slow to open), a minute and a half after when
/// they cannot be opened (the last is not waited out), and one whose
/// connection broke, a minute after. The give-up sentence says which.
pub(crate) const STALL_RECONNECTS: u32 = 3;

/// Block read at a time when a part on disk is hashed, between two checks
/// for a cancel.
const HASH_BLOCK_BYTES: usize = 1024 * 1024;

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

    /// The server answered with something that cannot be used, and asking
    /// again would get the same: a part that cannot be placed in the file,
    /// a redirect that goes nowhere.
    #[error("Network error: {0}")]
    Refused(String),

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

    /// One connection sent nothing for the quiet time. The transfer drops it
    /// and carries on over another; this never leaves [`download_file`].
    #[error("the connection sent nothing for {0:?}")]
    Quiet(Duration),

    /// A connection described a different file from the one the part holds
    /// the start of. The transfer starts the file over; this never leaves
    /// [`download_file`].
    #[error("{0}")]
    Changed(String),

    /// Nothing arrived over [`STALL_RECONNECTS`] reconnects in a row, so the
    /// download stopped. The part is kept for the next try.
    #[error("no data arrived for {quiet:?}; {kept} bytes kept")]
    Stalled {
        /// Bytes of the file on disk.
        kept: u64,
        /// How long nothing arrived.
        quiet: Duration,
    },
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
    /// The commit a fetch over a copy on disk (an upgrade) is pinned to. Its
    /// bytes land at `<dest>.<commit>.part`, so trying the same upgrade
    /// again carries on from them, and no other revision's part is taken
    /// for its start.
    pub commit: Option<&'a str>,
    /// Bearer token for private repositories.
    pub token: Option<&'a str>,
    /// Expected size from `HuggingFace` metadata, when known.
    pub expected_size: Option<u64>,
    /// Sink for the transfer's readings.
    pub progress: Option<RawCallback>,
    /// Cancellation token. A cancelled transfer leaves its `.part` file behind
    /// so the next attempt resumes rather than restarting.
    pub cancel: Option<CancellationToken>,
    /// Sink for notes that stand in for progress while there is none.
    pub notice: Option<NoticeCallback>,
    /// How long a connection may send nothing before it is dropped:
    /// [`STALL_AFTER`] in production, shorter in tests.
    pub stall_after: Duration,
}

/// Download one file, resuming if a previous attempt left bytes behind.
///
/// A connection that sends nothing for `req.stall_after` is dropped, and the
/// transfer carries on from the bytes on disk over a new one, up to
/// [`STALL_RECONNECTS`] times in a row with nothing arriving; then it fails
/// with [`NativeError::Stalled`], keeping the part. Once the download has
/// had bytes or a quiet connection, a connection that breaks or will not
/// open is lost the same way (see [`Lost::after`]).
///
/// On success the file exists at `req.dest` and no `.part` file remains. On a
/// verification failure the `.part` file is removed, because resuming onto
/// corrupt bytes would fail the same way forever.
pub(crate) async fn download_file(
    client: &Client,
    req: &NativeDownload<'_>,
) -> Result<(), NativeError> {
    let part_path = part_path_for(req.dest, req.commit);

    if let Some(parent) = req.dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| NativeError::io("create_dir", &e))?;
    }

    let mut carry = Carry {
        furthest: existing_len(&part_path),
        ..Carry::default()
    };
    let mut lost = Lost::default();
    let mut started_over = false;
    let outcome = loop {
        let resume_from = existing_len(&part_path);
        // A resume reads the part through the hasher before any connection
        // opens, so no connection sits unread while gigabytes are hashed: a
        // server may reset one it is not read from.
        if carry.to_hash(resume_from) {
            notify(
                req,
                &format!(
                    "checking the {} already downloaded…",
                    format_size(resume_from)
                ),
            );
            let hasher = hash_part(&part_path, resume_from, req.cancel.as_ref()).await?;
            carry.hashed = Some((hasher, resume_from));
        }
        let began = tokio::time::Instant::now();
        let cause = match stream_to_part(client, req, &part_path, resume_from, &mut carry).await {
            Ok(outcome) => break outcome,
            Err(NativeError::Quiet(_)) => Cause::Quiet,
            // A connection that did not open, or broke, once this download
            // has had bytes or a quiet connection: the network dropped for a
            // while, as Wi-Fi does. The first connection of a download that
            // has had nothing ends the download, spending no reconnects: the
            // host may simply be wrong.
            Err(NativeError::Network(why)) if carry.received > 0 || lost.in_a_row > 0 => {
                tracing::warn!(url = req.url, why, "download connection failed");
                Cause::Failed
            }
            // The file is not the one the part holds the start of: start it
            // over rather than splice two files, once.
            Err(NativeError::Changed(why)) if !started_over => {
                started_over = true;
                tracing::warn!(url = req.url, why, "file changed between connections");
                std::fs::remove_file(&part_path).map_err(|e| NativeError::io("remove", &e))?;
                carry = Carry {
                    received: carry.received,
                    ..Carry::default()
                };
                notify(req, &format!("{why}; starting it over…"));
                continue;
            }
            Err(NativeError::Changed(why)) => {
                return Err(NativeError::Refused(format!(
                    "{why} again while it downloaded"
                )));
            }
            Err(e) => return Err(e),
        };
        lost.after(req, &part_path, &mut carry, cause, began)
            .await?;
    };

    verify(&part_path, req.expected_size, outcome.digest.as_ref())?;

    std::fs::rename(&part_path, req.dest).map_err(|e| NativeError::io("rename", &e))?;

    Ok(())
}

/// What one connection hands the next, so a reconnect neither reads the part
/// through the hasher again, nor counts the bytes received from nothing,
/// nor appends the bytes of another file.
#[derive(Default)]
struct Carry {
    /// The hash of the part's first `.1` bytes, as the last connection left it.
    hashed: Option<(Sha256, u64)>,
    /// Bytes received from the network by the connections before this one.
    received: u64,
    /// The furthest the part has reached.
    furthest: u64,
    /// The file's digest, as the first connection that gave one gave it.
    digest: Option<String>,
    /// The file's size, as the first connection that gave one gave it.
    total: Option<u64>,
    /// A connection has answered with the file.
    answered: bool,
}

impl Carry {
    /// Take what this connection says of the file, or refuse it when that is
    /// not what an earlier connection said: the file changed on the server.
    fn same_file(
        &mut self,
        digest: Option<&String>,
        total: Option<u64>,
    ) -> Result<(), NativeError> {
        if let (Some(was), Some(now)) = (self.digest.as_ref(), digest) {
            if !was.eq_ignore_ascii_case(now) {
                return Err(NativeError::Changed(
                    "the file changed on the server".to_string(),
                ));
            }
        }
        if let (Some(was), Some(now)) = (self.total, total) {
            if was != now {
                return Err(NativeError::Changed(format!(
                    "the file on the server is now {now} bytes, not {was}"
                )));
            }
        }
        self.digest = self.digest.take().or_else(|| digest.cloned());
        self.total = self.total.or(total);
        self.answered = true;
        Ok(())
    }

    /// Whether a part of `len` bytes is to be read through the hasher before
    /// the next connection: it is not hashed to that length already, and no
    /// connection has said the file has no digest to check it against.
    fn to_hash(&self, len: u64) -> bool {
        let no_digest = self.answered && self.digest.is_none();
        len > 0 && !no_digest && self.hashed.as_ref().is_none_or(|(_, at)| *at != len)
    }
}

/// How a connection of a download in progress was lost.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Cause {
    /// It sent nothing for the quiet time.
    Quiet,
    /// It did not open, or broke: an error rather than silence.
    Failed,
}

/// The connections a download has lost in a row with the part getting no
/// further, and how long nothing has arrived over them.
#[derive(Default)]
struct Lost {
    /// Connections lost in a row; reconnects so far, as the row counts them.
    in_a_row: u32,
    /// What lost the first of them, which the row's note names.
    first: Option<Cause>,
    /// Quiet times that have passed over them with nothing arriving.
    windows: u32,
}

impl Lost {
    /// A connection was lost by `cause`, having begun at `began`. Count it,
    /// and say on the row that the download reconnects; or, past
    /// [`STALL_RECONNECTS`], end it with [`NativeError::Stalled`].
    ///
    /// Bytes past the furthest the part has reached start the count again.
    /// Bytes that only make up what a server that ignored the range threw
    /// away are no progress. A connection that failed having brought
    /// nothing is tried again only once a quiet time has passed since it
    /// began, so a dropped network has the reconnects' whole span to come
    /// back in, and the download gives up after about the time it says.
    async fn after(
        &mut self,
        req: &NativeDownload<'_>,
        part_path: &Path,
        carry: &mut Carry,
        cause: Cause,
        began: tokio::time::Instant,
    ) -> Result<(), NativeError> {
        let reached = existing_len(part_path);
        let progressed = reached > carry.furthest;
        if progressed {
            carry.furthest = reached;
            *self = Self::default();
        }
        self.in_a_row += 1;
        let first = *self.first.get_or_insert(cause);
        if cause == Cause::Quiet {
            self.windows += 1;
        }
        if self.in_a_row > STALL_RECONNECTS {
            return Err(NativeError::Stalled {
                kept: reached,
                quiet: req.stall_after * self.windows,
            });
        }

        let n = self.in_a_row;
        tracing::warn!(
            url = req.url,
            reconnect = n,
            "download connection lost; reconnecting"
        );
        let why = match first {
            Cause::Quiet => format!(
                "no data for {}",
                format_duration(Some(req.stall_after.as_secs_f64()))
            ),
            Cause::Failed => "connection lost".to_string(),
        };
        notify(
            req,
            &format!("{why}, reconnecting ({n} of {STALL_RECONNECTS})…"),
        );

        if cause == Cause::Failed && !progressed {
            let pause = tokio::time::sleep_until(began + req.stall_after);
            match req.cancel.as_ref() {
                Some(cancel) => tokio::select! {
                    biased;
                    () = cancel.cancelled() => return Err(NativeError::Cancelled),
                    () = pause => {}
                },
                None => pause.await,
            }
            self.windows += 1;
        }
        Ok(())
    }
}

fn notify(req: &NativeDownload<'_>, message: &str) {
    if let Some(notice) = req.notice.as_ref() {
        notice(message);
    }
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

/// Stream the response body of one connection into the `.part` file.
///
/// A connection that sends nothing for `req.stall_after`, before its headers
/// or between two chunks, ends in [`NativeError::Quiet`], with what arrived
/// flushed to the part and its hash and count left in `carry`.
async fn stream_to_part(
    client: &Client,
    req: &NativeDownload<'_>,
    part_path: &Path,
    resume_from: u64,
    carry: &mut Carry,
) -> Result<TransferOutcome, NativeError> {
    let (response, expected) = send_following_redirects(client, req, resume_from).await?;

    let status = response.status();

    // The server has all of it already: nothing left to transfer, go verify.
    if status == StatusCode::RANGE_NOT_SATISFIABLE && resume_from > 0 {
        return finish_without_transfer(req, part_path, expected).await;
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
    if appending {
        check_range(response.headers(), resume_from, req.expected_size)?;
    }

    let reported_total = total_size(response.headers(), start_at);
    carry.same_file(expected.as_ref(), reported_total)?;
    let total = reported_total.or(req.expected_size);

    // Network bytes count over every connection of this call, from what the
    // earlier ones received: a reconnect does not start the speed again.
    let received_before = carry.received;
    let report = |written: u64| {
        if let Some(cb) = req.progress.as_ref() {
            cb(RawProgress::new(
                written,
                received_before + (written - start_at),
                total,
            ));
        }
    };
    // The initial report is unconditional: it says where a resume stands.
    report(start_at);

    let carried = carry.hashed.take();
    let hasher = match (expected.is_some(), appending) {
        (false, _) => None,
        (true, false) => Some(Sha256::new()),
        (true, true) => Some(hasher_for_resume(req, part_path, resume_from, carried).await?),
    };

    let file = open_part(part_path, appending)?;

    // Writes and hashing happen on a dedicated blocking thread, fed through a
    // bounded channel, so network reads overlap disk I/O instead of
    // serializing with it — and no async worker thread ever blocks on a
    // syscall. Every exit path below drops `tx` and joins the sink, which
    // flushes whatever arrived; on errors and cancellation that keeps the
    // `.part` file resumable.
    let (tx, rx) = mpsc::channel::<bytes::Bytes>(SINK_QUEUE_CHUNKS);
    let mut sink = spawn_sink(file, hasher, rx);

    let mut downloaded = start_at;

    // Per-chunk reporting hammered the watch channel thousands of times a
    // second; consumers sample every 250ms anyway. 100ms keeps the final
    // display one tick fresh while shedding ~99% of the sends.
    let mut throttle = ProgressThrottle::default_interval();

    let mut stream = response.bytes_stream();
    loop {
        // Only the wait on the socket is timed: a send to the sink that
        // waits is a slow disk, not a quiet connection.
        let next = tokio::time::timeout(req.stall_after, stream.next());
        let chunk = if let Some(cancel) = req.cancel.as_ref() {
            tokio::select! {
                biased;
                () = cancel.cancelled() => {
                    drop(tx);
                    let _ = sink.await;
                    return Err(NativeError::Cancelled);
                }
                chunk = next => chunk,
            }
        } else {
            next.await
        };

        let lost = match chunk {
            Ok(Some(Ok(chunk))) => Ok(chunk),
            Ok(None) => break,
            // Quiet, or broken: either way the connection is lost.
            Err(_) => Err(NativeError::Quiet(req.stall_after)),
            Ok(Some(Err(e))) => Err(NativeError::Network(describe(&e))),
        };
        let chunk = match lost {
            Ok(chunk) => chunk,
            Err(lost) => {
                // Keep what arrived, its hash and its count for the next
                // connection. Dropping the stream drops this one.
                drop(stream);
                drop(tx);
                let hasher = join_sink(&mut sink).await?;
                report(downloaded);
                carry.hashed = hasher.map(|h| (h, downloaded));
                carry.received = received_before + (downloaded - start_at);
                return Err(lost);
            }
        };

        let len = chunk.len() as u64;
        if tx.send(chunk).await.is_err() {
            // The sink died; its join result carries the real I/O error.
            break;
        }

        downloaded += len;
        if throttle.should_emit() {
            report(downloaded);
        }
    }

    drop(tx);
    let hasher = join_sink(&mut sink).await?;

    // Unconditional final report so the last sample lands on the exact final
    // byte count instead of wherever the throttle last let one through.
    report(downloaded);

    Ok(TransferOutcome {
        digest: hasher.map(|h| Digested {
            expected: expected.unwrap_or_default(),
            actual: hex(&h.finalize()),
        }),
    })
}

/// The hasher for a resume from `resume_from`: the bytes already on disk are
/// part of the digest. The last connection's hash covers them when it
/// covered exactly these; otherwise they are read through the hasher, off
/// the async threads, with a note on the row while that runs.
async fn hasher_for_resume(
    req: &NativeDownload<'_>,
    part_path: &Path,
    resume_from: u64,
    carried: Option<(Sha256, u64)>,
) -> Result<Sha256, NativeError> {
    if let Some((hasher, len)) = carried {
        if len == resume_from {
            return Ok(hasher);
        }
    }
    notify(
        req,
        &format!(
            "checking the {} already downloaded…",
            format_size(resume_from)
        ),
    );
    hash_part(part_path, resume_from, req.cancel.as_ref()).await
}

/// A 206 must say where it starts, start where it was asked to, and, when
/// it says how long the file is and the size is known, say the same size.
/// Anything else would put its bytes at a place in the file nobody can
/// vouch for.
fn check_range(
    headers: &HeaderMap,
    asked: u64,
    expected_size: Option<u64>,
) -> Result<(), NativeError> {
    let range = headers.get(CONTENT_RANGE).and_then(|v| v.to_str().ok());
    let Some(start) = range.and_then(parse_content_range_start) else {
        return Err(NativeError::Refused(format!(
            "asked for the file from byte {asked}, and the server did not say where its \
             answer starts"
        )));
    };
    if start != asked {
        return Err(NativeError::Refused(format!(
            "asked for the file from byte {asked}, and the server sent it from byte {start}"
        )));
    }
    let total = range.and_then(parse_content_range_total);
    if let (Some(total), Some(expected)) = (total, expected_size) {
        if total != expected {
            return Err(NativeError::Refused(format!(
                "the server's file is {total} bytes, not the {expected} expected"
            )));
        }
    }
    Ok(())
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
        .connect_timeout(CONNECT_TIMEOUT)
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
        .map_err(|e| NativeError::Refused(format!("invalid URL {}: {e}", req.url)))?;
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

        let response = send_unless_cancelled(request, req.cancel.as_ref(), req.stall_after).await?;

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
                NativeError::Refused(format!("redirect from {url} carried no Location"))
            })?;

        url = url
            .join(location)
            .map_err(|e| NativeError::Refused(format!("bad redirect target {location}: {e}")))?;
    }

    Err(NativeError::TooManyRedirects(req.url.to_string()))
}

/// Send `request`, unless `cancel` fires first or the server answers
/// nothing for `stall_after`. A cancel while the server is still
/// deciding what to answer is then as prompt as one mid-transfer, and
/// nothing is on disk yet to flush.
async fn send_unless_cancelled(
    request: reqwest::RequestBuilder,
    cancel: Option<&CancellationToken>,
    stall_after: Duration,
) -> Result<Response, NativeError> {
    let send = async {
        // A connection that does not open fails by the client's own error,
        // its connect timeout running out before this clock does: only a
        // server that opened and then said nothing is quiet.
        tokio::time::timeout(stall_after, request.send())
            .await
            .map_err(|_| NativeError::Quiet(stall_after))?
            .map_err(|e| NativeError::Network(describe(&e)))
    };
    let Some(cancel) = cancel else {
        return send.await;
    };
    tokio::select! {
        biased;
        () = cancel.cancelled() => Err(NativeError::Cancelled),
        response = send => response,
    }
}

/// A transport error in plain words: what failed, then the cause at the
/// bottom of its chain (`could not connect to huggingface.co: Network is
/// unreachable (os error 51)`), rather than reqwest's own `error sending
/// request for url (…)`, which names the address and not what went wrong.
fn describe(e: &reqwest::Error) -> String {
    let mut cause: &dyn std::error::Error = e;
    while let Some(next) = cause.source() {
        cause = next;
    }
    let host = e
        .url()
        .and_then(Url::host_str)
        .unwrap_or("the server")
        .to_string();
    if e.is_connect() {
        format!("could not connect to {host}: {cause}")
    } else if e.is_body() || e.is_decode() {
        format!("the connection to {host} broke: {cause}")
    } else if std::ptr::addr_eq(cause, e) {
        e.to_string()
    } else {
        format!("the request to {host} failed: {cause}")
    }
}

/// Nothing to transfer (HTTP 416): hash whatever is on disk so verification can
/// still run against it.
async fn finish_without_transfer(
    req: &NativeDownload<'_>,
    part_path: &Path,
    expected: Option<String>,
) -> Result<TransferOutcome, NativeError> {
    let Some(expected) = expected else {
        return Ok(TransferOutcome { digest: None });
    };

    let len = existing_len(part_path);
    notify(
        req,
        &format!("checking the {} already downloaded…", format_size(len)),
    );
    let hasher = hash_part(part_path, len, req.cancel.as_ref()).await?;

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

/// Where the bytes of `dest` land until they are verified: `<dest>.part`, or
/// `<dest>.<commit>.part` for a fetch pinned to `commit`.
pub(super) fn part_path_for(dest: &Path, commit: Option<&str>) -> PathBuf {
    let mut name = dest.as_os_str().to_os_string();
    if let Some(tag) = commit.map(commit_tag).filter(|tag| !tag.is_empty()) {
        name.push(".");
        name.push(tag);
    }
    name.push(PART_SUFFIX);
    PathBuf::from(name)
}

/// A commit as it goes into a file name: its letters and digits.
fn commit_tag(commit: &str) -> String {
    commit.chars().filter(char::is_ascii_alphanumeric).collect()
}

/// Remove every part of `dest` but `keep`: `<dest>.part` and any
/// `<dest>.<commit>.part`. A fetch pinned to one commit carries on from its
/// own part only; another revision's bytes are no start for it.
pub(super) fn remove_other_parts(dest: &Path, keep: &Path) {
    let (Some(dir), Some(name)) = (dest.parent(), dest.file_name()) else {
        return;
    };
    let prefix = format!("{}.", name.to_string_lossy());
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let entry_name = entry.file_name();
        let entry_name = entry_name.to_string_lossy();
        let ours = entry_name.starts_with(&prefix) && entry_name.ends_with(PART_SUFFIX);
        if ours && entry.path() != keep {
            let _ = std::fs::remove_file(entry.path());
        }
    }
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

/// The hash of the first `len` bytes of `path`, read on a blocking thread.
///
/// Only ever runs on a resume, where the bytes already on disk are part of the
/// digest but were never streamed through the hasher. That is gigabytes on a
/// large model: read on an async thread, it held the thread that answers
/// `/health` for 35 s (#2061). A cancel stops the read between two blocks.
async fn hash_part(
    path: &Path,
    len: u64,
    cancel: Option<&CancellationToken>,
) -> Result<Sha256, NativeError> {
    let path = path.to_path_buf();
    let cancel = cancel.cloned();
    tokio::task::spawn_blocking(move || {
        let mut hasher = Sha256::new();
        seed_hasher(&path, len, &mut hasher, cancel.as_ref())?;
        Ok(hasher)
    })
    .await
    .map_err(|e| NativeError::Io {
        operation: "read".to_string(),
        message: format!("hashing thread panicked: {e}"),
    })?
}

/// Feed the first `len` bytes of `path` through `hasher`, stopping with
/// [`NativeError::Cancelled`] once `cancel` fires. Blocking: see
/// [`hash_part`].
fn seed_hasher(
    path: &Path,
    len: u64,
    hasher: &mut Sha256,
    cancel: Option<&CancellationToken>,
) -> Result<(), NativeError> {
    let file = std::fs::File::open(path).map_err(|e| NativeError::io("open", &e))?;
    let mut reader = std::io::BufReader::with_capacity(HASH_BLOCK_BYTES, file.take(len));
    loop {
        // Called once before each block is read.
        #[cfg(test)]
        stall_tests::run_seed_probe(path);
        if cancel.is_some_and(CancellationToken::is_cancelled) {
            return Err(NativeError::Cancelled);
        }
        let block = reader.fill_buf().map_err(|e| NativeError::io("read", &e))?;
        if block.is_empty() {
            return Ok(());
        }
        hasher.update(block);
        let read = block.len();
        reader.consume(read);
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

/// Pull the start out of `bytes <start>-<end>/<total>`.
fn parse_content_range_start(value: &str) -> Option<u64> {
    value
        .trim()
        .strip_prefix("bytes ")?
        .split('-')
        .next()?
        .trim()
        .parse::<u64>()
        .ok()
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
pub(crate) mod tests;

#[cfg(test)]
#[path = "native_stall_tests.rs"]
mod stall_tests;
