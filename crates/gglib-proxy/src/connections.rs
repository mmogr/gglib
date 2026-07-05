//! In-memory registry of active `/v1/chat/completions` connections.
//!
//! [`ActiveConnectionsRegistry`] tracks every in-flight request — both
//! direct chat completions and orchestrator/council virtual-model requests
//! (see [`crate::council_proxy`]) — so the proxy dashboard can show live
//! connection state: which model is being served, how long it has been
//! running, and prompt-processing progress during the pre-fill phase.
//!
//! ## RAII cleanup
//!
//! [`ActiveConnectionsRegistry::register`] returns a [`ConnectionGuard`].
//! Its [`Drop`] impl unregisters the connection unconditionally:
//!
//! * **Normal completion** — the guard falls out of scope at the end of the
//!   handler / spawned task.
//! * **Early return** (`?`, `return Err(...)`) — Rust runs `Drop` for every
//!   local on every return path, so there is no code path that "forgets" to
//!   clean up.
//! * **Client disconnect** — Axum drops the response body's underlying
//!   future (or the `tokio::spawn`ed streaming task is aborted) when the
//!   connection closes, which drops everything the future/task owns,
//!   including the guard.
//! * **Panic** — this workspace does not set `panic = "abort"` in any
//!   profile, so a panicking task unwinds its stack and runs `Drop` impls
//!   (including the guard's) before `tokio::spawn`'s `JoinHandle` observes
//!   the panic. No stale entry can survive a panicking handler.
//!
//! The guard holds its own `Arc` clone of the registry (rather than
//! borrowing `AppState`), so it can be moved freely into spawned tasks and
//! outlive the handler stack frame that created it.
//!
//! ## Concurrency design
//!
//! Uses `std::sync::Mutex` — not `tokio::sync::Mutex` — following the
//! [`crate::metrics::ContextMetricsStore`] convention: every critical
//! section is a handful of synchronous map operations with no `.await`
//! inside, so it is impossible at the type level to hold the lock across an
//! await point.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use uuid::Uuid;

// =============================================================================
// ConnectionPhase
// =============================================================================

/// Lifecycle phase of an active connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionPhase {
    /// Registered; waiting for llama.cpp to assign a slot and begin prefill.
    Queued,
    /// llama.cpp is processing the prompt (pre-fill). `prompt_*` fields on
    /// [`ActiveConnectionSnapshot`] are updated as progress frames arrive.
    ProcessingPrompt,
    /// The model has started emitting generated tokens.
    Generating,
}

// =============================================================================
// ActiveConnectionSnapshot
// =============================================================================

/// Point-in-time, serializable view of one active connection.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ActiveConnectionSnapshot {
    /// Unique id assigned at registration.
    pub id: Uuid,
    /// Name of the model (or virtual council model) serving this connection.
    pub model_name: String,
    /// Unix timestamp (seconds since epoch) when the connection was registered.
    pub started_at_secs: u64,
    /// `true` for a streaming (SSE) request, `false` for non-streaming.
    pub is_streaming: bool,
    /// Effective context size in use, when known (`None` for the council
    /// virtual-model path, which has no single per-request context size).
    pub num_ctx: Option<u64>,
    /// Current lifecycle phase.
    pub phase: ConnectionPhase,
    /// Tokens processed so far, from the most recent prompt-progress frame.
    pub prompt_processed: Option<u32>,
    /// Total tokens in the prompt, from the most recent prompt-progress frame.
    pub prompt_total: Option<u32>,
    /// Tokens served from the KV cache, from the most recent progress frame.
    pub prompt_cached: Option<u32>,
    /// Wall-clock milliseconds elapsed, from the most recent progress frame.
    pub prompt_time_ms: Option<u64>,
}

/// Internal mutable record. Never exposed directly — always projected
/// through [`ActiveConnection::snapshot`] into an [`ActiveConnectionSnapshot`].
struct ActiveConnection {
    model_name: String,
    started_at_secs: u64,
    is_streaming: bool,
    num_ctx: Option<u64>,
    phase: ConnectionPhase,
    prompt_processed: Option<u32>,
    prompt_total: Option<u32>,
    prompt_cached: Option<u32>,
    prompt_time_ms: Option<u64>,
}

impl ActiveConnection {
    fn snapshot(&self, id: Uuid) -> ActiveConnectionSnapshot {
        ActiveConnectionSnapshot {
            id,
            model_name: self.model_name.clone(),
            started_at_secs: self.started_at_secs,
            is_streaming: self.is_streaming,
            num_ctx: self.num_ctx,
            phase: self.phase,
            prompt_processed: self.prompt_processed,
            prompt_total: self.prompt_total,
            prompt_cached: self.prompt_cached,
            prompt_time_ms: self.prompt_time_ms,
        }
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// =============================================================================
// ActiveConnectionsRegistry
// =============================================================================

/// Thread-safe registry of active proxy connections.
///
/// Wrap in `Arc` and share across Axum handler tasks. [`Self::register`]
/// takes `&Arc<Self>` (not `&self`) so the returned [`ConnectionGuard`] can
/// hold its own `Arc` clone and unregister on drop without borrowing
/// `AppState`.
#[derive(Default)]
pub struct ActiveConnectionsRegistry {
    connections: Mutex<HashMap<Uuid, ActiveConnection>>,
}

impl ActiveConnectionsRegistry {
    /// Create an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a new active connection and return its RAII guard.
    ///
    /// The connection starts in [`ConnectionPhase::Queued`]. Dropping the
    /// returned [`ConnectionGuard`] unregisters it — see the module-level
    /// documentation for the exhaustive list of paths this covers.
    #[must_use]
    pub fn register(
        self: &Arc<Self>,
        model_name: impl Into<String>,
        is_streaming: bool,
        num_ctx: Option<u64>,
    ) -> ConnectionGuard {
        let id = Uuid::new_v4();
        let connection = ActiveConnection {
            model_name: model_name.into(),
            started_at_secs: now_secs(),
            is_streaming,
            num_ctx,
            phase: ConnectionPhase::Queued,
            prompt_processed: None,
            prompt_total: None,
            prompt_cached: None,
            prompt_time_ms: None,
        };

        {
            let mut guard = self.connections.lock().unwrap_or_else(|e| e.into_inner());
            guard.insert(id, connection);
        }

        ConnectionGuard {
            id,
            registry: Arc::clone(self),
        }
    }

    /// Record prompt-processing progress for `id`, moving it into
    /// [`ConnectionPhase::ProcessingPrompt`].
    ///
    /// A no-op if `id` is not (or no longer) present.
    pub fn update_progress(&self, id: Uuid, processed: u32, total: u32, cached: u32, time_ms: u64) {
        let mut guard = self.connections.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(conn) = guard.get_mut(&id) {
            conn.phase = ConnectionPhase::ProcessingPrompt;
            conn.prompt_processed = Some(processed);
            conn.prompt_total = Some(total);
            conn.prompt_cached = Some(cached);
            conn.prompt_time_ms = Some(time_ms);
        }
    }

    /// Move `id` into [`ConnectionPhase::Generating`].
    ///
    /// Called once the first generated token (or any non-progress stream
    /// event) arrives. A no-op if `id` is absent, and idempotent if called
    /// repeatedly for the same connection.
    pub fn mark_generating(&self, id: Uuid) {
        let mut guard = self.connections.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(conn) = guard.get_mut(&id) {
            conn.phase = ConnectionPhase::Generating;
        }
    }

    /// Snapshot every currently active connection.
    #[must_use]
    pub fn snapshot(&self) -> Vec<ActiveConnectionSnapshot> {
        let guard = self.connections.lock().unwrap_or_else(|e| e.into_inner());
        guard.iter().map(|(id, conn)| conn.snapshot(*id)).collect()
    }

    /// Number of currently active connections.
    #[must_use]
    pub fn len(&self) -> usize {
        self.connections
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .len()
    }

    /// `true` if there are no active connections.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// `true` if any connection *other than* `exclude` is actively occupying
    /// the upstream — i.e. past the `Queued` phase (prompt processing or
    /// generating).
    ///
    /// Used to distinguish a slow-because-busy upstream (another request holds
    /// the single llama-server slot) from a genuinely wedged one, so the
    /// streaming keepalive path does not treat a legitimate queue wait as a
    /// degradation strike.
    #[must_use]
    pub fn others_busy(&self, exclude: Uuid) -> bool {
        let guard = self.connections.lock().unwrap_or_else(|e| e.into_inner());
        guard.iter().any(|(id, conn)| {
            *id != exclude
                && matches!(
                    conn.phase,
                    ConnectionPhase::ProcessingPrompt | ConnectionPhase::Generating
                )
        })
    }

    /// Remove `id` from the registry. Only called from [`ConnectionGuard`]'s
    /// `Drop` impl.
    fn remove(&self, id: Uuid) {
        self.connections
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&id);
    }
}

// =============================================================================
// ConnectionGuard
// =============================================================================

/// RAII guard returned by [`ActiveConnectionsRegistry::register`].
///
/// See the module-level documentation for the cleanup guarantees this
/// provides on drop, early return, disconnect, and panic.
pub struct ConnectionGuard {
    id: Uuid,
    registry: Arc<ActiveConnectionsRegistry>,
}

impl ConnectionGuard {
    /// The connection id assigned at registration.
    #[must_use]
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Record prompt-processing progress for this connection.
    pub fn update_progress(&self, processed: u32, total: u32, cached: u32, time_ms: u64) {
        self.registry
            .update_progress(self.id, processed, total, cached, time_ms);
    }

    /// Mark this connection as generating (post-prefill).
    pub fn mark_generating(&self) {
        self.registry.mark_generating(self.id);
    }

    /// `true` if any *other* in-flight connection is actively occupying the
    /// upstream slot (prompt processing or generating).
    ///
    /// A `true` result means "the upstream is busy serving someone else",
    /// which the streaming keepalive path treats as a legitimate queue wait
    /// rather than a degradation.
    #[must_use]
    pub fn others_active(&self) -> bool {
        self.registry.others_busy(self.id)
    }
}

impl std::fmt::Debug for ConnectionGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConnectionGuard")
            .field("id", &self.id)
            .finish()
    }
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        self.registry.remove(self.id);
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_adds_entry_in_queued_phase() {
        let registry = Arc::new(ActiveConnectionsRegistry::new());
        let guard = registry.register("test-model", true, Some(8192));

        assert_eq!(registry.len(), 1);
        let snapshots = registry.snapshot();
        assert_eq!(snapshots.len(), 1);
        let snap = &snapshots[0];
        assert_eq!(snap.id, guard.id());
        assert_eq!(snap.model_name, "test-model");
        assert!(snap.is_streaming);
        assert_eq!(snap.num_ctx, Some(8192));
        assert_eq!(snap.phase, ConnectionPhase::Queued);
        assert_eq!(snap.prompt_processed, None);
    }

    #[test]
    fn update_progress_transitions_phase_and_sets_fields() {
        let registry = Arc::new(ActiveConnectionsRegistry::new());
        let guard = registry.register("test-model", true, None);

        guard.update_progress(50, 100, 10, 250);

        let snapshots = registry.snapshot();
        let snap = &snapshots[0];
        assert_eq!(snap.phase, ConnectionPhase::ProcessingPrompt);
        assert_eq!(snap.prompt_processed, Some(50));
        assert_eq!(snap.prompt_total, Some(100));
        assert_eq!(snap.prompt_cached, Some(10));
        assert_eq!(snap.prompt_time_ms, Some(250));
    }

    #[test]
    fn mark_generating_transitions_phase() {
        let registry = Arc::new(ActiveConnectionsRegistry::new());
        let guard = registry.register("test-model", true, None);

        guard.update_progress(100, 100, 0, 500);
        guard.mark_generating();

        let snapshots = registry.snapshot();
        assert_eq!(snapshots[0].phase, ConnectionPhase::Generating);
    }

    #[test]
    fn others_active_ignores_self_and_queued_peers() {
        let registry = Arc::new(ActiveConnectionsRegistry::new());
        let a = registry.register("m", true, None);
        let b = registry.register("m", true, None);

        // Both are in Queued phase → neither counts as busy for the other.
        assert!(!a.others_active());
        assert!(!b.others_active());

        // b starts processing its prompt → now a sees a busy peer, but b
        // (looking at itself + queued a) does not.
        b.update_progress(10, 100, 0, 5);
        assert!(a.others_active());
        assert!(!b.others_active());

        // A generating peer also counts as busy.
        b.mark_generating();
        assert!(a.others_active());

        // Once b drops, a is alone again.
        drop(b);
        assert!(!a.others_active());
    }

    #[test]
    fn guard_drop_removes_entry() {
        let registry = Arc::new(ActiveConnectionsRegistry::new());
        let guard = registry.register("test-model", false, Some(4096));
        assert_eq!(registry.len(), 1);

        drop(guard);

        assert_eq!(registry.len(), 0);
        assert!(registry.snapshot().is_empty());
    }

    #[test]
    fn guard_drop_on_early_return_still_cleans_up() {
        let registry = Arc::new(ActiveConnectionsRegistry::new());

        fn do_work(registry: &Arc<ActiveConnectionsRegistry>, fail: bool) -> Result<(), ()> {
            let _guard = registry.register("test-model", false, None);
            if fail {
                return Err(());
            }
            Ok(())
        }

        let _ = do_work(&registry, true);
        assert_eq!(registry.len(), 0, "guard must be dropped on early return");
    }

    #[test]
    fn guard_drop_on_panic_still_cleans_up() {
        let registry = Arc::new(ActiveConnectionsRegistry::new());
        let registry_for_panic = Arc::clone(&registry);

        let result = std::panic::catch_unwind(move || {
            let _guard = registry_for_panic.register("test-model", true, None);
            panic!("simulated handler panic");
        });

        assert!(result.is_err());
        assert_eq!(
            registry.len(),
            0,
            "guard must be dropped during unwind on panic"
        );
    }

    #[test]
    fn operations_on_missing_id_are_noops() {
        let registry = ActiveConnectionsRegistry::new();
        let bogus_id = Uuid::new_v4();

        // None of these should panic even though `bogus_id` was never registered.
        registry.update_progress(bogus_id, 1, 2, 3, 4);
        registry.mark_generating(bogus_id);

        assert!(registry.is_empty());
    }

    #[test]
    fn concurrent_register_and_drop_leaves_registry_consistent() {
        let registry = Arc::new(ActiveConnectionsRegistry::new());
        let mut handles = Vec::new();

        for _ in 0..8 {
            let registry = Arc::clone(&registry);
            handles.push(std::thread::spawn(move || {
                let guard = registry.register("test-model", true, Some(2048));
                guard.update_progress(1, 2, 0, 10);
                // Guard drops at the end of the closure.
            }));
        }

        for handle in handles {
            handle.join().expect("worker thread panicked");
        }

        assert_eq!(
            registry.len(),
            0,
            "all connections must be cleaned up after their guards drop"
        );
    }

    #[test]
    fn multiple_connections_snapshot_independently() {
        let registry = Arc::new(ActiveConnectionsRegistry::new());
        let guard_a = registry.register("model-a", true, Some(4096));
        let guard_b = registry.register("model-b", false, None);

        guard_a.update_progress(10, 20, 5, 100);

        let snapshots = registry.snapshot();
        assert_eq!(snapshots.len(), 2);

        let snap_a = snapshots
            .iter()
            .find(|s| s.id == guard_a.id())
            .expect("model-a snapshot present");
        let snap_b = snapshots
            .iter()
            .find(|s| s.id == guard_b.id())
            .expect("model-b snapshot present");

        assert_eq!(snap_a.model_name, "model-a");
        assert_eq!(snap_a.phase, ConnectionPhase::ProcessingPrompt);
        assert_eq!(snap_b.model_name, "model-b");
        assert_eq!(snap_b.phase, ConnectionPhase::Queued);
    }
}
