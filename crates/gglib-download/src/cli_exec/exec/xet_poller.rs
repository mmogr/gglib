//! File-stat fallback for hf-xet downloads.
//!
//! `huggingface_hub` does not drive `tqdm` when the underlying transfer is
//! handled by the `hf_xet` Rust client, so [`JsonProgressBar`] (in the Python
//! helper) never receives `update()` calls. Without intervention the Rust
//! bridge would only ever see the initial `progress {0,0}` event followed
//! eventually by `complete` — leaving the user staring at a frozen
//! `0 B/0 B` progress bar for the entire transfer.
//!
//! This module spawns a small Tokio task that periodically `stat`s the
//! destination files and emits a synthetic `(downloaded, total)` event
//! whenever no real progress event has arrived for [`STALE_AFTER`]. Real
//! events take precedence: callers must invoke [`XetPoller::note_real_event`]
//! whenever a Python-side `Progress` message is observed, and the poller
//! stays dormant for as long as those events keep flowing.
//!
//! The poller has no knowledge of the Python protocol or process lifecycle —
//! it is a pure stat-and-callback utility that the [`super::python_bridge`]
//! layer composes.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio::time::Instant;

use super::python_bridge::ProgressCallback;

/// How often the poller wakes up to stat the destination files.
const POLL_INTERVAL: Duration = Duration::from_millis(250);

/// Minimum age of the most recent real event before synthetic events kick in.
///
/// While Python-side `tqdm` updates are flowing, the poller stays silent.
/// Once real events go quiet for longer than this threshold the poller
/// assumes hf-xet is driving the transfer directly and starts reporting
/// on-disk byte counts.
const STALE_AFTER: Duration = Duration::from_secs(1);

/// Background file-stat fallback that emits synthetic progress events when
/// the Python helper goes silent (typical of the hf-xet fast path).
pub struct XetPoller {
    /// Last time a real Python `Progress` event was observed.
    last_real_event_at: Arc<Mutex<Instant>>,
    /// Last synthetic byte count we reported. Used to suppress no-change emits.
    last_synthetic_bytes: Arc<AtomicU64>,
    /// Handle to the polling task; aborted on [`XetPoller::shutdown`].
    handle: JoinHandle<()>,
}

impl XetPoller {
    /// Spawn a background poller for the given destination files.
    ///
    /// `targets` are absolute paths (`dest_root.join(file)` for each file the
    /// downloader was asked to fetch). `expected_total`, when known, is
    /// forwarded as the `total` field on synthetic progress events; pass
    /// `None` if the size is unknown (the bar will display downloaded bytes
    /// only).
    ///
    /// The poller invokes `on_progress(downloaded_sum, expected_total)`
    /// whenever the on-disk size changes **and** no real event has been
    /// reported for at least [`STALE_AFTER`].
    pub fn spawn(
        targets: Vec<PathBuf>,
        expected_total: Option<u64>,
        on_progress: ProgressCallback,
    ) -> Self {
        let last_real_event_at = Arc::new(Mutex::new(Instant::now()));
        let last_synthetic_bytes = Arc::new(AtomicU64::new(0));

        let last_real = Arc::clone(&last_real_event_at);
        let last_bytes = Arc::clone(&last_synthetic_bytes);
        let total = expected_total.unwrap_or(0);

        let handle = tokio::spawn(async move {
            let mut tick = tokio::time::interval(POLL_INTERVAL);
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            // The first tick fires immediately — skip it so we don't race the
            // child process to its first byte on disk.
            tick.tick().await;

            loop {
                tick.tick().await;

                if !is_stale(&last_real).await {
                    continue;
                }

                let downloaded = sum_sizes(&targets).await;
                if downloaded == 0 {
                    // Nothing on disk yet — wait for the next tick.
                    continue;
                }
                if downloaded == last_bytes.load(Ordering::Relaxed) {
                    continue;
                }

                last_bytes.store(downloaded, Ordering::Relaxed);
                on_progress(downloaded, total);
            }
        });

        Self {
            last_real_event_at,
            last_synthetic_bytes,
            handle,
        }
    }

    /// Record that a real Python `Progress` event was just observed.
    ///
    /// This bumps the staleness timer back to "now", causing the poller to
    /// stay dormant for as long as real events keep arriving regularly. Safe
    /// to call from synchronous code that is already inside a Tokio runtime.
    pub fn note_real_event(&self) {
        // Optimistic non-blocking update. If the lock is contended (the
        // poller task is holding it for a stale-check) the next call will
        // pick up the slack — at worst we miss one staleness reset, which
        // is harmless.
        if let Ok(mut guard) = self.last_real_event_at.try_lock() {
            *guard = Instant::now();
        }
    }

    /// Stop the background poller. Idempotent; consumes the handle.
    pub fn shutdown(self) {
        self.handle.abort();
        // Reset counters so the type behaves as a one-shot resource.
        self.last_synthetic_bytes.store(0, Ordering::Relaxed);
    }
}

/// Returns `true` when the most recent real event is older than [`STALE_AFTER`].
async fn is_stale(last_real: &Arc<Mutex<Instant>>) -> bool {
    let guard = last_real.lock().await;
    guard.elapsed() >= STALE_AFTER
}

/// Sum the on-disk sizes of every existing target.
///
/// Each target may be either a file or a directory:
/// * **File** — its length is added directly. Missing files contribute zero
///   (they may simply not have been created yet).
/// * **Directory** — every regular file beneath it is summed recursively.
///   This is required for the hf-xet path because `huggingface_hub` writes
///   bytes to `<dest>/.cache/huggingface/download/<filename>.incomplete`
///   while the transfer is in flight and only renames to `<dest>/<filename>`
///   on completion. Stat'ing the final path alone would report 0 B for the
///   entire transfer.
async fn sum_sizes(targets: &[PathBuf]) -> u64 {
    let mut total: u64 = 0;
    for path in targets {
        let Ok(metadata) = tokio::fs::metadata(path).await else {
            continue;
        };
        if metadata.is_file() {
            total = total.saturating_add(metadata.len());
        } else if metadata.is_dir() {
            total = total.saturating_add(sum_dir_size(path).await);
        }
    }
    total
}

/// Recursively sum the sizes of every regular file under `dir`.
///
/// Symlinks are followed for size accounting only when the entry's metadata
/// reports `is_file()` (i.e. the symlink points at a regular file). Errors
/// reading individual entries are silently treated as zero so a transient
/// `EACCES` or a vanishing temp file doesn't poison the running total.
async fn sum_dir_size(dir: &Path) -> u64 {
    let mut total: u64 = 0;
    let mut stack: Vec<PathBuf> = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(mut entries) = tokio::fs::read_dir(&current).await else {
            continue;
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let Ok(metadata) = entry.metadata().await else {
                continue;
            };
            if metadata.is_file() {
                total = total.saturating_add(metadata.len());
            } else if metadata.is_dir() {
                stack.push(entry.path());
            }
        }
    }
    total
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;
    use tokio::io::AsyncWriteExt;

    type ProgressLog = Arc<StdMutex<Vec<(u64, u64)>>>;

    /// Build a `ProgressCallback` that records every `(downloaded, total)`
    /// pair into a shared vector.
    fn recording_callback() -> (ProgressCallback, ProgressLog) {
        let log = Arc::new(StdMutex::new(Vec::new()));
        let log_clone = Arc::clone(&log);
        let cb: ProgressCallback = Arc::new(move |d, t| {
            log_clone.lock().unwrap().push((d, t));
        });
        (cb, log)
    }

    #[tokio::test(flavor = "current_thread")]
    async fn emits_synthetic_progress_when_python_is_silent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("model.gguf");
        tokio::fs::write(&path, b"").await.unwrap();

        let (cb, log) = recording_callback();
        let poller = XetPoller::spawn(vec![path.clone()], Some(1024), cb);

        // Wait past the staleness threshold without any real events arriving,
        // then grow the file and let the poller observe the change.
        tokio::time::sleep(STALE_AFTER + Duration::from_millis(50)).await;

        let mut f = tokio::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .await
            .unwrap();
        f.write_all(&[0u8; 512]).await.unwrap();
        f.flush().await.unwrap();
        drop(f);

        // Give the poller a couple of stat ticks to notice.
        tokio::time::sleep(POLL_INTERVAL * 3).await;

        poller.shutdown();

        let entries = log.lock().unwrap().clone();
        assert!(
            entries.iter().any(|(d, t)| *d == 512 && *t == 1024),
            "expected synthetic event with downloaded=512 total=1024, got {entries:?}"
        );
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn stays_dormant_while_real_events_are_flowing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("model.gguf");
        tokio::fs::write(&path, vec![0u8; 4096]).await.unwrap();

        let (cb, log) = recording_callback();
        let poller = XetPoller::spawn(vec![path.clone()], Some(8192), cb);

        // Simulate a steady stream of real Python events. Each call resets
        // the staleness timer well before STALE_AFTER elapses.
        for _ in 0..6 {
            poller.note_real_event();
            tokio::time::advance(Duration::from_millis(300)).await;
            tokio::task::yield_now().await;
        }

        poller.shutdown();

        assert!(
            log.lock().unwrap().is_empty(),
            "expected no synthetic events while real events were flowing"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn coalesces_repeated_no_change_polls() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("model.gguf");
        tokio::fs::write(&path, vec![0u8; 2048]).await.unwrap();

        let (cb, log) = recording_callback();
        let poller = XetPoller::spawn(vec![path.clone()], None, cb);

        // Wait past staleness + several poll intervals.
        tokio::time::sleep(STALE_AFTER + POLL_INTERVAL * 5).await;

        poller.shutdown();

        // The file size never changes after the first observation, so we
        // should see at most one synthetic event regardless of how many
        // poll intervals elapse.
        let entries = log.lock().unwrap().clone();
        assert!(
            entries.len() <= 1,
            "expected ≤1 synthetic event for a static file, got {entries:?}"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn walks_directory_targets_recursively() {
        // Mirrors the real hf-xet layout: bytes live under
        // `<dest>/.cache/huggingface/download/<file>.incomplete` while the
        // transfer is in flight, and only the directory itself exists at
        // the spawned target path.
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join(".cache/huggingface/download");
        tokio::fs::create_dir_all(&cache).await.unwrap();
        let temp_file = cache.join("model.gguf.incomplete");
        tokio::fs::write(&temp_file, vec![0u8; 4096]).await.unwrap();

        let (cb, log) = recording_callback();
        let poller = XetPoller::spawn(vec![dir.path().to_path_buf()], Some(8192), cb);

        // Wait past the staleness threshold so synthetic events kick in.
        tokio::time::sleep(STALE_AFTER + Duration::from_millis(50)).await;

        // Grow the in-flight temp file; the poller should observe the
        // change via the recursive walk even though the final path
        // (`<dest>/model.gguf`) doesn't exist yet.
        let mut f = tokio::fs::OpenOptions::new()
            .append(true)
            .open(&temp_file)
            .await
            .unwrap();
        f.write_all(&[0u8; 2048]).await.unwrap();
        f.flush().await.unwrap();
        drop(f);

        tokio::time::sleep(POLL_INTERVAL * 3).await;

        poller.shutdown();

        let entries = log.lock().unwrap().clone();
        assert!(
            entries.iter().any(|(d, t)| *d == 6144 && *t == 8192),
            "expected synthetic event with downloaded=6144 total=8192, got {entries:?}"
        );
    }
}
