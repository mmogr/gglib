//! Semaphore-gated KV cache lifecycle: restore before generation, save after.
//!
//! The semaphore permit covers the ENTIRE restore→forward→save cycle without
//! release (Design Directive 1). For non-streaming requests the permit is held
//! inline; for streaming it is moved into the spawned task (Step 4).

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use dashmap::DashSet;
use reqwest::Client;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::time::{Duration, sleep};
use tracing::{debug, warn};

use crate::slots::{self, SlotIoResult};

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
    pub clear_all_pending: Arc<AtomicBool>,
    pub per_session_cleared: Arc<DashSet<String>>,
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

    let mut result = slots::restore_slot(&config.client, &config.base_url, &sanitized).await;

    // Retry only transient failures (UpstreamDead / timeout / network error)
    if matches!(result, SlotIoResult::Transient(_)) {
        for attempt in 1..=MAX_RETRIES {
            debug!(
                "retry restore for {session_id} (attempt {}/{})",
                attempt, MAX_RETRIES
            );
            sleep(RETRY_BACKOFF).await;
            result = slots::restore_slot(&config.client, &config.base_url, &sanitized).await;
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
        sanitized_session_id,
        &config.clear_all_pending,   // Arc<AtomicBool> → &AtomicBool
        &config.per_session_cleared, // Arc<DashSet<String>> → &DashSet<String>
    )
    .await;
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

    // Restore
    let restore_result = restore_with_retry(config, &sanitized).await;

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

    // Restore (permit held)
    let restore_result = restore_with_retry(config, &sanitized).await;

    Ok((permit, sanitized, restore_result))
}

/// Clear slot files for a session (or all sessions if None).
///
/// Deliberately does NOT acquire the semaphore (Design A) — clears are instant
/// from CLI/GUI with no spinner. The cleared flag prevents subsequent saves.
pub async fn clear_cache(config: &StreamConfig, session_id: Option<&str>) -> std::io::Result<()> {
    // NO semaphore acquire — Design A: instant clear
    let result = slots::clear_slot_files(&config.slot_dir, session_id).await;

    if let Some(id) = session_id
        && let Ok(sanitized) = slots::sanitize_session_id(id)
    {
        config.per_session_cleared.insert(sanitized);
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
            clear_all_pending: Arc::new(AtomicBool::new(false)),
            per_session_cleared: Arc::new(DashSet::new()),
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
            clear_all_pending: Arc::new(AtomicBool::new(false)),
            per_session_cleared: Arc::new(DashSet::new()),
        };

        let _ = clear_cache(&config, Some("test_session")).await;
        assert!(config.per_session_cleared.contains("test_session"));
    }

    #[tokio::test]
    async fn test_restore_removes_flags_unconditionally() {
        let config = StreamConfig {
            client: Client::new(),
            base_url: "http://127.0.0.1:0".to_string(), // Non-existent server
            slot_dir: PathBuf::from("/tmp/test-slots"),
            clear_all_pending: Arc::new(AtomicBool::new(true)), // Simulate pending global clear
            per_session_cleared: Arc::new(DashSet::new()),
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

    #[tokio::test]
    async fn test_prepare_streaming_cycle_rejects_bad_session_id() {
        let config = StreamConfig {
            client: Client::new(),
            base_url: "http://localhost:8080".to_string(),
            slot_dir: PathBuf::from("/tmp/test-slots"),
            clear_all_pending: Arc::new(AtomicBool::new(false)),
            per_session_cleared: Arc::new(DashSet::new()),
        };
        let gate = Arc::new(Semaphore::new(1));

        // Path traversal attempt — should return Err without touching semaphore
        let result = prepare_streaming_cycle(&config, gate.clone(), "../evil").await;
        assert!(result.is_err());

        // Semaphore still has 1 permit available (was never acquired)
        assert_eq!(gate.available_permits(), 1);
    }
}
