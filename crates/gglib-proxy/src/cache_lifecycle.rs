//! Semaphore-gated KV cache lifecycle: restore before generation, save after.
//!
//! The semaphore permit covers the ENTIRE restore→forward→save cycle without
//! release (Design Directive 1). For non-streaming requests the permit is held
//! inline; for streaming it is moved into the spawned task (Step 4).

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use dashmap::DashSet;
use reqwest::Client;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::time::{Duration, sleep};
use tracing::{debug, warn};

use crate::slots::{self, SlotIoResult};

/// Composite key for the hot-cache bypass: (model_id, session_id).
/// Both fields must match to consider a session "hot" in RAM.
#[derive(Clone, Debug)]
pub struct LastLoadedSession {
    pub model_id: u32,
    pub session_id: String,
}

/// Maximum number of retry attempts for pre-generation restore failures.
/// Total attempts = 1 (initial) + MAX_RETRIES = 3.
const MAX_RETRIES: u32 = 2;

/// Backoff between retry attempts (100ms).
const RETRY_BACKOFF: Duration = Duration::from_millis(100);

/// Owned configuration bundle for cache lifecycle operations.
///
/// Holds `Arc`-wrapped shared state so it can be cloned and moved across
/// `tokio::spawn` boundaries. Deliberately does NOT hold the semaphore —
/// that is an AppState concurrency control, passed as `&Semaphore`.
#[derive(Clone)]
pub struct StreamConfig {
    pub client: Client,
    pub base_url: String,
    pub slot_dir: PathBuf,
    /// Database ID of the model whose slots are being cached.
    /// Used to namespace slot files under `{slot_dir}/{model_id}/`.
    pub model_id: u32,
    pub clear_all_pending: Arc<AtomicBool>,
    pub per_session_cleared: Arc<DashSet<String>>,
    /// Unix timestamp (seconds) when the current llama-server process started.
    /// Used by mtime guard to skip restoring stale slot files.
    pub server_start_time: Arc<AtomicU64>,
    /// Last session successfully loaded into RAM (hot in KV cache).
    /// Composite key (model_id + session_id) used to bypass disk restore
    /// when the same model+session is already hot.
    pub last_loaded_session: Arc<tokio::sync::RwLock<Option<LastLoadedSession>>>,
}

/// Restore KV cache for a session, with retry on transient failures.
///
/// Always removes the per-session cleared flag AND resets the global clear
/// flag afterward (unconditional — Bug 5 fix). This prevents the global
/// clear deadlock: once a restore attempt occurs (success or failure), the
/// system is back to "live" state and should accept future saves.
pub async fn restore_with_retry(config: &StreamConfig, session_id: &str) -> SlotIoResult {
    let sanitized = match slots::sanitize_session_id(session_id) {
        Ok(s) => s,
        Err(e) => return SlotIoResult::Permanent(e),
    };

    // Existence precheck: if no slot file exists at all, skip the network
    // restore call entirely. llama-server returns HTTP 400 (not 404) for a
    // missing/invalid slot file, which `restore_slot` classifies as
    // `Transient` — without this check, every first-ever restore for a
    // session (first turn of a conversation, or the first request after a
    // restart) would be retried `MAX_RETRIES` times and logged as a failure
    // for what is actually the expected "nothing cached yet" case.
    let path = slots::slot_bin_path(&config.slot_dir, config.model_id, &sanitized);
    let file_exists = tokio::fs::metadata(&path).await.is_ok();

    // mtime guard: skip restoring a slot file written by a prior llama-server
    // instance (see `slots::slot_file_is_stale` for the fail-open contract).
    let server_start_secs = config.server_start_time.load(Ordering::SeqCst);

    let mut result = if !file_exists {
        SlotIoResult::NotFound
    } else {
        let is_stale = slots::slot_file_is_stale(
            &config.slot_dir,
            config.model_id,
            &sanitized,
            server_start_secs,
        )
        .await;

        if is_stale {
            debug!(
                "skipping restore for {session_id} — slot file predates current server instance"
            );
            SlotIoResult::NotFound
        } else {
            slots::restore_slot(
                &config.client,
                &config.base_url,
                &config.slot_dir,
                config.model_id,
                &sanitized,
            )
            .await
        }
    };

    // Retry only transient failures (UpstreamDead / timeout / network error)
    if matches!(result, SlotIoResult::Transient(_)) {
        for attempt in 1..=MAX_RETRIES {
            debug!(
                "retry restore for {session_id} (attempt {}/{})",
                attempt, MAX_RETRIES
            );
            sleep(RETRY_BACKOFF).await;
            result = slots::restore_slot(
                &config.client,
                &config.base_url,
                &config.slot_dir,
                config.model_id,
                &sanitized,
            )
            .await;
            if !matches!(result, SlotIoResult::Transient(_)) {
                break;
            }
        }
    }

    // UNCONDITIONAL after every restore attempt (success, NotFound, exhausted, permanent):
    // 1. Remove per-session cleared flag — session is now "live" again
    // 2. Reset global clear flag — prevents deadlock where clear_all_pending
    //    stays true forever, blocking all saves across all sessions
    config.per_session_cleared.remove(&sanitized);
    config.clear_all_pending.swap(false, Ordering::SeqCst);

    match &result {
        SlotIoResult::Ok => debug!("restored KV cache for {session_id}"),
        SlotIoResult::NotFound => {
            debug!("no cached slot for {session_id} — proceeding cold")
        }
        SlotIoResult::Transient(e) => {
            warn!("restore failed for {session_id} after retries: {e} — degrading to cold start")
        }
        SlotIoResult::Permanent(e) => {
            warn!("restore permanently failed for {session_id}: {e}")
        }
    }

    result
}

/// Save KV cache after generation completes. Awaited (not detached).
///
/// Takes the already-sanitized session ID — both calling paths (streaming and
/// non-streaming) sanitize once at cycle start, so this avoids redundant work.
pub async fn save_after_generation(config: &StreamConfig, sanitized_session_id: &str) {
    slots::attempt_save(
        &config.client,
        &config.base_url,
        &config.slot_dir,
        config.model_id,
        sanitized_session_id,
        &config.clear_all_pending,   // Arc<AtomicBool> → &AtomicBool
        &config.per_session_cleared, // Arc<DashSet<String>> → &DashSet<String>
    )
    .await;

    // Mark this session as hot in RAM — next request for the same session
    // can skip the disk restore.
    *config.last_loaded_session.write().await = Some(LastLoadedSession {
        model_id: config.model_id,
        session_id: sanitized_session_id.to_string(),
    });
}

/// Non-streaming cache lifecycle: acquire permit, restore→generate→save, release.
///
/// The semaphore permit is held across the ENTIRE cycle (Design Directive 1).
/// Sanitization happens BEFORE acquire so bad session IDs never enter the gate.
pub async fn run_with_cache<F, Fut, T>(
    config: &StreamConfig,
    slot_gate: &Semaphore,
    session_id: &str,
    generation_work: F,
) -> Result<(T, SlotIoResult), SlotIoResult>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = T>,
{
    // Sanitize BEFORE acquiring the semaphore — 400 Bad Request on failure
    let sanitized = match slots::sanitize_session_id(session_id) {
        Ok(s) => s,
        Err(e) => return Err(SlotIoResult::Permanent(e)),
    };

    // Acquire permit — held for entire cycle (no release until save completes)
    let _permit = slot_gate.acquire().await.unwrap();

    // Hot cache bypass: skip disk restore if session is already in RAM
    let is_hot = {
        let last = config.last_loaded_session.read().await;
        last.as_ref().map(|l| (l.model_id, l.session_id.as_str()))
            == Some((config.model_id, sanitized.as_str()))
    };

    let restore_result = if is_hot {
        debug!(
            "Session {} is already hot in RAM — skipping disk restore",
            sanitized
        );
        SlotIoResult::Ok
    } else {
        restore_with_retry(config, &sanitized).await
    };

    // Generation work (semaphore held — prevents interleaving corruption)
    let result = generation_work().await;

    // Save (awaited before permit drops) — passes known-good sanitized ID
    save_after_generation(config, &sanitized).await;

    // Permit dropped at end of scope — entire cycle protected
    // Fail-open: restore failure is logged but does NOT abort generation.
    // The response was already produced; we return it regardless of cache state.
    match &restore_result {
        SlotIoResult::Ok => tracing::debug!("Cache restored for session {}", sanitized),
        SlotIoResult::NotFound => {
            tracing::info!("No cached slot for session {} — full re-prefill", sanitized)
        }
        SlotIoResult::Transient(msg) => tracing::warn!(
            "Transient cache restore failure for session {}: {}",
            sanitized,
            msg
        ),
        SlotIoResult::Permanent(msg) => {
            tracing::warn!("Cache restore failed for session {}: {}", sanitized, msg)
        }
    }
    Ok((result, restore_result))
}

/// Streaming cache lifecycle: acquire permit, restore, return permit for spawn.
///
/// The caller (Step 4: `sse_stream::spawn_and_return`) receives the
/// `OwnedSemaphorePermit` and moves it into the spawned task, where it is
/// held across generation→save→drop.
///
/// Returns `Err` immediately on sanitization failure — no semaphore touched.
pub async fn prepare_streaming_cycle(
    config: &StreamConfig,
    slot_gate: Arc<Semaphore>,
    session_id: &str,
) -> Result<(OwnedSemaphorePermit, String, SlotIoResult), SlotIoResult> {
    // Sanitize BEFORE acquiring the semaphore — short-circuit on failure
    let sanitized = match slots::sanitize_session_id(session_id) {
        Ok(s) => s,
        Err(e) => return Err(SlotIoResult::Permanent(e)),
    };

    // Acquire owned permit — will be moved into spawned task.
    // `acquire_owned` takes `self`, so we consume the Arc<Semaphore> here.
    let permit = slot_gate.acquire_owned().await.unwrap();

    // Hot cache bypass: skip disk restore if session is already in RAM
    let is_hot = {
        let last = config.last_loaded_session.read().await;
        last.as_ref().map(|l| (l.model_id, l.session_id.as_str()))
            == Some((config.model_id, sanitized.as_str()))
    };

    let restore_result = if is_hot {
        debug!(
            "Session {} is already hot in RAM — skipping disk restore",
            sanitized
        );
        SlotIoResult::Ok
    } else {
        restore_with_retry(config, &sanitized).await
    };

    Ok((permit, sanitized, restore_result))
}

/// Clear slot files for a session (or all sessions if None).
///
/// Deliberately does NOT acquire the semaphore (Design A) — clears are instant
/// from CLI/GUI with no spinner. The cleared flag prevents subsequent saves.
///
/// Without a flag, a cycle already mid-generation when the clear runs (started
/// before the clear, holding the slot permit, unaffected by Design A's no-op
/// semaphore) would have its `save_after_generation` re-write the very file
/// this call just deleted. Setting `clear_all_pending` for the global case
/// closes that race: `attempt_save` checks it before writing, and the next
/// cycle's `restore_with_retry` clears it again once it's no longer needed.
pub async fn clear_cache(config: &StreamConfig, session_id: Option<&str>) -> std::io::Result<()> {
    // NO semaphore acquire — Design A: instant clear
    let result = slots::clear_slot_files(&config.slot_dir, session_id).await;

    match session_id {
        Some(id) => {
            if let Ok(sanitized) = slots::sanitize_session_id(id) {
                config.per_session_cleared.insert(sanitized.clone());
                // Invalidate hot cache if the cleared session was the one in RAM
                let mut last = config.last_loaded_session.write().await;
                if last.as_ref().map(|l| (l.model_id, l.session_id.as_str()))
                    == Some((config.model_id, sanitized.as_str()))
                {
                    *last = None;
                }
            }
        }
        None => {
            config.clear_all_pending.store(true, Ordering::SeqCst);
            // Global clear invalidates the hot cache entirely
            *config.last_loaded_session.write().await = None;
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stream_config_is_clone() {
        let config = StreamConfig {
            client: Client::new(),
            base_url: "http://localhost:8080".to_string(),
            slot_dir: PathBuf::from("/tmp/slots"),
            model_id: 0,
            clear_all_pending: Arc::new(AtomicBool::new(false)),
            per_session_cleared: Arc::new(DashSet::new()),
            server_start_time: Arc::new(AtomicU64::new(0)),
            last_loaded_session: Arc::new(tokio::sync::RwLock::new(None)),
        };
        let _clone = config.clone();
    }

    #[test]
    fn test_max_retries_constant() {
        assert_eq!(MAX_RETRIES, 2);
    }

    #[tokio::test]
    async fn test_clear_cache_sets_per_session_flag() {
        let config = StreamConfig {
            client: Client::new(),
            base_url: "http://localhost:8080".to_string(),
            slot_dir: PathBuf::from("/tmp/test-slots"),
            model_id: 0,
            clear_all_pending: Arc::new(AtomicBool::new(false)),
            per_session_cleared: Arc::new(DashSet::new()),
            server_start_time: Arc::new(AtomicU64::new(0)),
            last_loaded_session: Arc::new(tokio::sync::RwLock::new(None)),
        };

        let _ = clear_cache(&config, Some("test_session")).await;
        assert!(config.per_session_cleared.contains("test_session"));
    }

    /// Regression test for the race where a global clear (no session id) set
    /// no guard at all, letting a concurrent in-flight generation's save
    /// silently resurrect the file the clear just deleted.
    #[tokio::test]
    async fn test_clear_cache_with_no_session_sets_clear_all_pending() {
        let config = StreamConfig {
            client: Client::new(),
            base_url: "http://localhost:8080".to_string(),
            slot_dir: PathBuf::from("/tmp/test-slots"),
            model_id: 0,
            clear_all_pending: Arc::new(AtomicBool::new(false)),
            per_session_cleared: Arc::new(DashSet::new()),
            server_start_time: Arc::new(AtomicU64::new(0)),
            last_loaded_session: Arc::new(tokio::sync::RwLock::new(None)),
        };

        let _ = clear_cache(&config, None).await;
        assert!(config.clear_all_pending.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_restore_removes_flags_unconditionally() {
        let config = StreamConfig {
            client: Client::new(),
            base_url: "http://127.0.0.1:0".to_string(), // Non-existent server
            slot_dir: PathBuf::from("/tmp/test-slots"),
            model_id: 0,
            clear_all_pending: Arc::new(AtomicBool::new(true)), // Simulate pending global clear
            per_session_cleared: Arc::new(DashSet::new()),
            server_start_time: Arc::new(AtomicU64::new(0)), // 0 → fail-open (always proceed)
            last_loaded_session: Arc::new(tokio::sync::RwLock::new(None)),
        };

        config
            .per_session_cleared
            .insert("test_session".to_string());
        assert!(config.per_session_cleared.contains("test_session"));
        assert!(config.clear_all_pending.load(Ordering::SeqCst));

        // Restore will fail (no server), but both flags are reset unconditionally
        let _ = restore_with_retry(&config, "test_session").await;

        // Per-session flag removed
        assert!(!config.per_session_cleared.contains("test_session"));
        // Global clear flag reset — prevents deadlock
        assert!(!config.clear_all_pending.load(Ordering::SeqCst));
    }

    /// Regression test for the existence precheck: a session with no slot
    /// file at all (first turn of a conversation, or first request after a
    /// restart) must never reach the network restore call. Proven by
    /// timing, same as the staleness-guard test below — a real call to a
    /// refused port would retry `MAX_RETRIES` times with `RETRY_BACKOFF`
    /// between them (200ms+); the existence check short-circuits to
    /// `NotFound` immediately instead, avoiding llama-server's HTTP 400
    /// "missing slot file" response being misclassified as `Transient` and
    /// retried for what is actually the expected cold-start case.
    #[tokio::test]
    async fn test_restore_with_retry_skips_missing_slot_file() {
        let dir = tempfile::tempdir().unwrap();
        // No file ever written for this session — dir doesn't even exist yet.
        let config = StreamConfig {
            client: Client::new(),
            base_url: "http://127.0.0.1:0".to_string(), // refused if ever called
            slot_dir: dir.path().to_path_buf(),
            model_id: 0,
            clear_all_pending: Arc::new(AtomicBool::new(false)),
            per_session_cleared: Arc::new(DashSet::new()),
            server_start_time: Arc::new(AtomicU64::new(0)),
            last_loaded_session: Arc::new(tokio::sync::RwLock::new(None)),
        };

        let started = tokio::time::Instant::now();
        let result = restore_with_retry(&config, "never-cached-session").await;
        let elapsed = started.elapsed();

        assert!(
            matches!(result, SlotIoResult::NotFound),
            "missing slot file should be treated as NotFound, got {:?}",
            result
        );
        assert!(
            elapsed < Duration::from_millis(150),
            "existence check should short-circuit before any network retry loop, took {:?}",
            elapsed
        );
    }

    /// Regression test for the mtime guard: a slot file written before the
    /// current llama-server instance started must never reach the network
    /// restore call. Proven by timing — a real call to a refused port would
    /// retry MAX_RETRIES times with RETRY_BACKOFF between them (200ms+); the
    /// guard short-circuits to `NotFound` immediately instead.
    #[tokio::test]
    async fn test_restore_with_retry_skips_stale_slot_file() {
        let dir = tempfile::tempdir().unwrap();
        let session_id = "stale-session";
        std::fs::write(
            slots::slot_bin_path(dir.path(), 0, session_id),
            b"old kv state",
        )
        .unwrap();

        // Any timestamp after the file's real mtime marks it stale.
        let server_start_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 3600;

        let config = StreamConfig {
            client: Client::new(),
            base_url: "http://127.0.0.1:0".to_string(), // refused if ever called
            slot_dir: dir.path().to_path_buf(),
            model_id: 0,
            clear_all_pending: Arc::new(AtomicBool::new(false)),
            per_session_cleared: Arc::new(DashSet::new()),
            server_start_time: Arc::new(AtomicU64::new(server_start_secs)),
            last_loaded_session: Arc::new(tokio::sync::RwLock::new(None)),
        };

        let started = tokio::time::Instant::now();
        let result = restore_with_retry(&config, session_id).await;
        let elapsed = started.elapsed();

        assert!(
            matches!(result, SlotIoResult::NotFound),
            "stale slot file should be treated as NotFound, got {:?}",
            result
        );
        assert!(
            elapsed < Duration::from_millis(150),
            "guard should short-circuit before any network retry loop, took {:?}",
            elapsed
        );
    }

    /// A fresh slot file (mtime after server start) must NOT be skipped by
    /// the guard — this exercises the "not stale" branch specifically, as
    /// opposed to the `server_start_secs == 0` fail-open branch already
    /// covered by `test_restore_removes_flags_unconditionally`.
    #[tokio::test]
    async fn test_restore_with_retry_does_not_skip_fresh_slot_file() {
        let dir = tempfile::tempdir().unwrap();
        let session_id = "fresh-session";
        std::fs::write(slots::slot_bin_path(dir.path(), 0, session_id), b"kv state").unwrap();

        // Server "started" long before the file was written.
        let server_start_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            .saturating_sub(3600);

        let config = StreamConfig {
            client: Client::new(),
            base_url: "http://127.0.0.1:0".to_string(), // refused — proves the call was attempted
            slot_dir: dir.path().to_path_buf(),
            model_id: 0,
            clear_all_pending: Arc::new(AtomicBool::new(false)),
            per_session_cleared: Arc::new(DashSet::new()),
            server_start_time: Arc::new(AtomicU64::new(server_start_secs)),
            last_loaded_session: Arc::new(tokio::sync::RwLock::new(None)),
        };

        let result = restore_with_retry(&config, session_id).await;

        // Not skipped by the guard — the real (failing, connection-refused)
        // network path was taken, which surfaces as Transient after retries.
        assert!(
            matches!(result, SlotIoResult::Transient(_)),
            "fresh slot file should reach the real restore call, got {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_prepare_streaming_cycle_rejects_bad_session_id() {
        let config = StreamConfig {
            client: Client::new(),
            base_url: "http://localhost:8080".to_string(),
            slot_dir: PathBuf::from("/tmp/test-slots"),
            model_id: 0,
            clear_all_pending: Arc::new(AtomicBool::new(false)),
            per_session_cleared: Arc::new(DashSet::new()),
            server_start_time: Arc::new(AtomicU64::new(0)),
            last_loaded_session: Arc::new(tokio::sync::RwLock::new(None)),
        };
        let gate = Arc::new(Semaphore::new(1));

        // Path traversal attempt — should return Err without touching semaphore
        let result = prepare_streaming_cycle(&config, gate.clone(), "../evil").await;
        assert!(result.is_err());

        // Semaphore still has 1 permit available (was never acquired)
        assert_eq!(gate.available_permits(), 1);
    }
}
