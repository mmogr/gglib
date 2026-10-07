//! Semaphore-gated KV cache lifecycle: restore before generation, save after.
//!
//! The semaphore permit covers the ENTIRE restore→forward→save cycle without
//! release. For non-streaming requests the permit is held inline; for
//! streaming it is moved into the spawned task.

use std::path::PathBuf;
use std::sync::Arc;

use reqwest::Client;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::time::sleep;
use tracing::{debug, warn};

use crate::slot_cache_state::SlotCacheState;
use crate::slots::{self, SlotIoResult};

// Retry budget for pre-generation restore failures — shared with
// `slots::attempt_save`'s retry loop; see `slots::MAX_RETRIES` for why.
// Total attempts = 1 (initial) + MAX_RETRIES = 3.
use slots::{MAX_RETRIES, RETRY_BACKOFF};

/// Owned configuration bundle for cache lifecycle operations.
///
/// Holds the shared state behind an `Arc` so it can be cloned and moved across
/// `tokio::spawn` boundaries. Deliberately does NOT hold the semaphore —
/// that is an `AppState` concurrency control, passed as `&Semaphore`.
#[derive(Clone)]
pub struct StreamConfig {
    pub client: Client,
    pub base_url: String,
    pub slot_dir: PathBuf,
    /// Database ID of the model whose slots are being cached.
    /// Used to namespace slot files via the flat `{model_id}__{session}.bin`
    /// filename prefix (see `gglib_core::paths::slot_file_name`).
    pub model_id: u32,
    /// What the proxy remembers of the slot cache between requests: the one
    /// value every request, the clear route and the load route share.
    pub(crate) state: Arc<SlotCacheState>,
}

#[cfg(any(test, feature = "test-support"))]
impl StreamConfig {
    /// A config with a slot cache state of its own, whose server started at
    /// `started`: for a test that drives the lifecycle without a proxy
    /// around it. The epoch leaves the mtime guard uninitialised, so it
    /// fails open.
    #[must_use]
    pub fn standalone(
        client: Client,
        base_url: String,
        slot_dir: PathBuf,
        model_id: u32,
        started: std::time::SystemTime,
    ) -> Self {
        Self {
            client,
            base_url,
            slot_dir,
            model_id,
            state: Arc::new(SlotCacheState::new(started)),
        }
    }
}

/// Restore KV cache for a session, with retry on transient failures.
///
/// Always tells the state a restore was attempted, whatever came of it (see
/// [`SlotCacheState::restore_attempted`]): once one has been, the session is
/// back to "live" and its saves are accepted again.
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
    let server_start_secs = config.state.server_start_secs();

    let mut result = if file_exists {
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
    } else {
        SlotIoResult::NotFound
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

    // UNCONDITIONAL after every restore attempt (success, NotFound, exhausted, permanent).
    config.state.restore_attempted(&sanitized);

    match &result {
        SlotIoResult::Ok => debug!("restored KV cache for {session_id}"),
        SlotIoResult::NotFound => {
            debug!("no cached slot for {session_id} — proceeding cold");
        }
        SlotIoResult::Transient(e) => {
            warn!("restore failed for {session_id} after retries: {e} — degrading to cold start");
        }
        SlotIoResult::Permanent(e) => {
            warn!("restore permanently failed for {session_id}: {e}");
        }
    }

    result
}

/// Save KV cache after generation completes. Awaited (not detached).
///
/// Skipped for a session cleared since its restore: the clear never waits for
/// the slot permit, so a cycle that was generating when it ran would otherwise
/// write back the file it had just deleted.
///
/// Takes the already-sanitized session ID — both calling paths (streaming and
/// non-streaming) sanitize once at cycle start, so this avoids redundant work.
pub(crate) async fn save_after_generation(config: &StreamConfig, sanitized_session_id: &str) {
    if config.state.may_save(sanitized_session_id) {
        slots::attempt_save(
            &config.client,
            &config.base_url,
            &config.slot_dir,
            config.model_id,
            sanitized_session_id,
        )
        .await;
    } else {
        debug!("skipping save for {sanitized_session_id} — cleared since its restore");
    }

    // This session is what the server holds in RAM now, saved or not. See
    // `SlotCacheState::mark_hot` for which session that leaves hot.
    config.state.mark_hot(config.model_id, sanitized_session_id);
}

/// Non-streaming cache lifecycle: acquire permit, restore→generate→save, release.
///
/// The semaphore permit is held across the ENTIRE cycle.
/// Sanitization happens BEFORE acquire so bad session IDs never enter the gate.
pub(crate) async fn run_with_cache<F, Fut, T>(
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
    let restore_result = if config.state.is_hot(config.model_id, &sanitized) {
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
            tracing::info!("No cached slot for session {} — full re-prefill", sanitized);
        }
        SlotIoResult::Transient(msg) => tracing::warn!(
            "Transient cache restore failure for session {}: {}",
            sanitized,
            msg
        ),
        SlotIoResult::Permanent(msg) => {
            tracing::warn!("Cache restore failed for session {}: {}", sanitized, msg);
        }
    }
    Ok((result, restore_result))
}

/// Streaming cache lifecycle: acquire permit, restore, return permit for spawn.
///
/// The caller (`sse_stream::spawn_and_return`) receives the
/// `OwnedSemaphorePermit` and moves it into the spawned task, where it is
/// held across generation→save→drop.
///
/// Returns `Err` immediately on sanitization failure — no semaphore touched.
pub(crate) async fn prepare_streaming_cycle(
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
    let restore_result = if config.state.is_hot(config.model_id, &sanitized) {
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

/// Resolve the `(permit, config, session_id)` triple
/// [`crate::forward::forward_chat_completion`] needs for a streaming forward
/// attempt. Fails open:
/// any [`prepare_streaming_cycle`] error degrades to `(None, None, None)` —
/// the caller proceeds without disk cache participation for this cycle.
pub(crate) async fn resolve_cache_triple(
    cfg: &StreamConfig,
    slot_gate: Arc<Semaphore>,
    session_id: &str,
) -> (
    Option<OwnedSemaphorePermit>,
    Option<StreamConfig>,
    Option<String>,
) {
    let sid = session_id.to_owned();
    match prepare_streaming_cycle(cfg, slot_gate, &sid).await {
        Ok((permit, _sanitized, _restore_result)) => (Some(permit), Some(cfg.clone()), Some(sid)),
        Err(_) => (None, None, None), // fail-open — proceed without disk cache for this cycle
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use tokio::time::Duration;

    /// A config for model 0 whose server started at `started`.
    fn config(base_url: &str, slot_dir: &Path, started: SystemTime) -> StreamConfig {
        let slot_dir = slot_dir.to_path_buf();
        StreamConfig::standalone(Client::new(), base_url.to_string(), slot_dir, 0, started)
    }

    #[test]
    fn test_stream_config_is_clone() {
        let config = config("http://localhost:8080", Path::new("/tmp/slots"), UNIX_EPOCH);
        let _clone = config.clone();
    }

    #[test]
    fn test_max_retries_constant() {
        assert_eq!(MAX_RETRIES, 2);
    }

    #[tokio::test]
    async fn test_restore_removes_flags_unconditionally() {
        // Non-existent server.
        let config = config(
            "http://127.0.0.1:0",
            Path::new("/tmp/test-slots"),
            UNIX_EPOCH,
        );

        // A pending global clear, and this session cleared too.
        config.state.clear_all();
        config.state.clear_session("test_session");
        assert!(!config.state.may_save("test_session"));
        assert!(!config.state.may_save("another_session"));

        // Restore will fail (no server), but both flags are reset unconditionally
        let _ = restore_with_retry(&config, "test_session").await;

        // Per-session flag removed
        assert!(config.state.may_save("test_session"));
        // Global clear flag reset — prevents deadlock
        assert!(config.state.may_save("another_session"));
    }

    /// A save the state forbids never reaches the server, and leaves nothing
    /// hot: not the session, whose next request has to restore, and not the
    /// one hot before it, which the server no longer holds.
    #[tokio::test]
    async fn a_cleared_sessions_save_is_skipped_and_leaves_nothing_hot() {
        // Refused if ever called: three tries and two backoffs, 200ms+.
        let config = config("http://127.0.0.1:0", Path::new("/tmp/slots"), UNIX_EPOCH);
        config.state.mark_hot(0, "coder");
        config.state.clear_session("planner");

        let started = tokio::time::Instant::now();
        save_after_generation(&config, "planner").await;
        let elapsed = started.elapsed();

        assert!(
            elapsed < Duration::from_millis(150),
            "a cleared session's save should not reach the network, took {elapsed:?}"
        );
        assert!(!config.state.is_hot(0, "planner"));
        assert!(!config.state.is_hot(0, "coder"));
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
        // The server is refused if ever called.
        let config = config("http://127.0.0.1:0", dir.path(), UNIX_EPOCH);

        let started = tokio::time::Instant::now();
        let result = restore_with_retry(&config, "never-cached-session").await;
        let elapsed = started.elapsed();

        assert!(
            matches!(result, SlotIoResult::NotFound),
            "missing slot file should be treated as NotFound, got {result:?}"
        );
        assert!(
            elapsed < Duration::from_millis(150),
            "existence check should short-circuit before any network retry loop, took {elapsed:?}"
        );
    }

    /// Regression test for the mtime guard: a slot file written before the
    /// current llama-server instance started must never reach the network
    /// restore call. Proven by timing — a real call to a refused port would
    /// retry `MAX_RETRIES` times with `RETRY_BACKOFF` between them (200ms+); the
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

        // Any timestamp after the file's real mtime marks it stale. The
        // server is refused if ever called.
        let server_start = SystemTime::now() + Duration::from_hours(1);
        let config = config("http://127.0.0.1:0", dir.path(), server_start);

        let started = tokio::time::Instant::now();
        let result = restore_with_retry(&config, session_id).await;
        let elapsed = started.elapsed();

        assert!(
            matches!(result, SlotIoResult::NotFound),
            "stale slot file should be treated as NotFound, got {result:?}"
        );
        assert!(
            elapsed < Duration::from_millis(150),
            "guard should short-circuit before any network retry loop, took {elapsed:?}"
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

        // Server "started" long before the file was written. It is refused,
        // which proves the call was attempted.
        let server_start = SystemTime::now() - Duration::from_hours(1);
        let config = config("http://127.0.0.1:0", dir.path(), server_start);

        let result = restore_with_retry(&config, session_id).await;

        // Not skipped by the guard — the real (failing, connection-refused)
        // network path was taken, which surfaces as Transient after retries.
        assert!(
            matches!(result, SlotIoResult::Transient(_)),
            "fresh slot file should reach the real restore call, got {result:?}"
        );
    }

    #[tokio::test]
    async fn test_prepare_streaming_cycle_rejects_bad_session_id() {
        let config = config(
            "http://localhost:8080",
            Path::new("/tmp/test-slots"),
            UNIX_EPOCH,
        );
        let gate = Arc::new(Semaphore::new(1));

        // Path traversal attempt — should return Err without touching semaphore
        let result = prepare_streaming_cycle(&config, gate.clone(), "../evil").await;
        assert!(result.is_err());

        // Semaphore still has 1 permit available (was never acquired)
        assert_eq!(gate.available_permits(), 1);
    }
}
