//! In-memory metrics store for the proxy pipeline.
//!
//! [`ContextMetricsStore`] is a fixed-capacity ring buffer that records one
//! [`ContextSnapshot`] per handled `/v1/chat/completions` request.  It is
//! the sole data source for the `GET /v1/proxy/status` endpoint, which
//! provides the shared data contract consumed by the CLI TUI and web
//! dashboard.
//!
//! ## Concurrency design
//!
//! [`ContextMetricsStore`] uses `std::sync::Mutex` — not `tokio::sync::Mutex`
//! — so that [`ContextMetricsStore::record`] can be a synchronous `fn`.  This
//! makes it **impossible** to hold the lock across an `.await` point at the
//! type level.  The critical section inside `record` is three lines: push,
//! conditional pop, done.  There is no I/O or allocation inside the lock.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

// =============================================================================
// Constants
// =============================================================================

/// Maximum number of [`ContextSnapshot`] entries retained in the ring buffer.
/// When the buffer is full the oldest entry is discarded to make room.
const MAX_SNAPSHOTS: usize = 50;

// =============================================================================
// ContextSnapshot
// =============================================================================

/// A single per-request observation recorded after the truncation pass.
#[derive(Debug, Clone)]
pub struct ContextSnapshot {
    /// Name of the model that was targeted by the request.
    pub model_name: String,
    /// Approximate payload size in bytes before any truncation.
    pub payload_chars_before: usize,
    /// Approximate payload size in bytes after truncation.  Equal to
    /// `payload_chars_before` when no changes were made.
    pub payload_chars_after: usize,
    /// Number of messages whose content was replaced with the truncation
    /// placeholder.
    pub messages_truncated: usize,
    /// `true` when the hard-abort budget check triggered and an HTTP 400 was
    /// returned to the client instead of forwarding the request.
    pub was_clamped: bool,
    /// Wall-clock time at which this snapshot was recorded.
    pub recorded_at: SystemTime,
}

// =============================================================================
// ContextMetricsStore
// =============================================================================

/// Thread-safe, fixed-capacity ring buffer of recent proxy request snapshots.
///
/// Wrap in `Arc` to share across Axum handler tasks:
///
/// ```rust,ignore
/// let store = Arc::new(ContextMetricsStore::new());
/// ```
pub struct ContextMetricsStore {
    /// Ring buffer of recent snapshots.  Protected by a *synchronous* mutex;
    /// see module documentation for the rationale.
    snapshots: Mutex<VecDeque<ContextSnapshot>>,
    /// Monotonically increasing count of all recorded requests, including
    /// those that were evicted from the ring buffer.
    total_requests: AtomicU64,
}

impl ContextMetricsStore {
    /// Create a new store with the default ring-buffer capacity
    /// ([`MAX_SNAPSHOTS`]).
    pub fn new() -> Self {
        Self {
            snapshots: Mutex::new(VecDeque::with_capacity(MAX_SNAPSHOTS)),
            total_requests: AtomicU64::new(0),
        }
    }

    /// Record a new snapshot.
    ///
    /// # Lock discipline
    ///
    /// This method is synchronous (`fn`, not `async fn`).  The mutex is
    /// acquired, the snapshot pushed, the oldest entry popped if the buffer
    /// is over capacity, and the lock dropped — all before returning.  No
    /// work is done inside the critical section that could block or allocate
    /// significantly.  The `total_requests` counter is updated with
    /// `Ordering::Relaxed`; exact ordering relative to concurrent readers is
    /// not required for a monotonic counter.
    pub fn record(&self, snapshot: ContextSnapshot) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);

        let mut guard = self.snapshots.lock().unwrap_or_else(|e| e.into_inner());
        guard.push_back(snapshot);
        if guard.len() > MAX_SNAPSHOTS {
            guard.pop_front();
        }
        // `guard` drops here — lock released.
    }

    /// Return up to `n` of the most recent snapshots in chronological order
    /// (oldest first within the returned slice).
    ///
    /// If the buffer contains fewer than `n` entries all of them are returned.
    pub fn recent(&self, n: usize) -> Vec<ContextSnapshot> {
        let guard = self.snapshots.lock().unwrap_or_else(|e| e.into_inner());
        let len = guard.len();
        let skip = len.saturating_sub(n);
        guard.iter().skip(skip).cloned().collect()
    }

    /// Total number of requests recorded since the store was created,
    /// including those that have been evicted from the ring buffer.
    pub fn total_requests(&self) -> u64 {
        self.total_requests.load(Ordering::Relaxed)
    }
}

impl Default for ContextMetricsStore {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_snapshot(model: &str) -> ContextSnapshot {
        ContextSnapshot {
            model_name: model.to_string(),
            payload_chars_before: 1_000,
            payload_chars_after: 800,
            messages_truncated: 1,
            was_clamped: false,
            recorded_at: SystemTime::now(),
        }
    }

    // ── Basic record + retrieve ───────────────────────────────────────────────

    #[test]
    fn record_single_snapshot_and_retrieve() {
        let store = ContextMetricsStore::new();
        store.record(make_snapshot("qwen-3b"));

        let recent = store.recent(10);
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].model_name, "qwen-3b");
        assert_eq!(recent[0].messages_truncated, 1);
        assert_eq!(store.total_requests(), 1);
    }

    #[test]
    fn recent_returns_at_most_n() {
        let store = ContextMetricsStore::new();
        for i in 0..10 {
            store.record(make_snapshot(&format!("model-{i}")));
        }
        let recent = store.recent(3);
        assert_eq!(recent.len(), 3);
        // Should be the last 3: model-7, model-8, model-9
        assert_eq!(recent[0].model_name, "model-7");
        assert_eq!(recent[2].model_name, "model-9");
    }

    #[test]
    fn recent_returns_all_when_fewer_than_n() {
        let store = ContextMetricsStore::new();
        store.record(make_snapshot("a"));
        store.record(make_snapshot("b"));

        let recent = store.recent(100);
        assert_eq!(recent.len(), 2);
    }

    #[test]
    fn empty_store_returns_empty_vec() {
        let store = ContextMetricsStore::new();
        assert!(store.recent(10).is_empty());
        assert_eq!(store.total_requests(), 0);
    }

    // ── Ring-buffer capacity ──────────────────────────────────────────────────

    #[test]
    fn ring_buffer_caps_at_max_snapshots() {
        let store = ContextMetricsStore::new();
        let insert_count = MAX_SNAPSHOTS + 5; // 55

        for i in 0..insert_count {
            store.record(make_snapshot(&format!("model-{i}")));
        }

        // total_requests must reflect all 55 inserts.
        assert_eq!(store.total_requests(), 55);

        // recent(MAX_SNAPSHOTS) must return exactly 50 entries (not 55).
        let recent = store.recent(MAX_SNAPSHOTS);
        assert_eq!(recent.len(), MAX_SNAPSHOTS);

        // The retained entries must be the LATEST 50 (indices 5..54).
        assert_eq!(recent[0].model_name, "model-5");
        assert_eq!(recent[MAX_SNAPSHOTS - 1].model_name, "model-54");
    }

    #[test]
    fn ring_buffer_exactly_at_capacity_does_not_evict() {
        let store = ContextMetricsStore::new();
        for i in 0..MAX_SNAPSHOTS {
            store.record(make_snapshot(&format!("m-{i}")));
        }
        assert_eq!(store.recent(MAX_SNAPSHOTS).len(), MAX_SNAPSHOTS);
        assert_eq!(store.total_requests(), MAX_SNAPSHOTS as u64);
    }

    // ── Counter ───────────────────────────────────────────────────────────────

    #[test]
    fn total_requests_increments_on_every_record() {
        let store = ContextMetricsStore::new();
        assert_eq!(store.total_requests(), 0);
        store.record(make_snapshot("a"));
        assert_eq!(store.total_requests(), 1);
        store.record(make_snapshot("b"));
        assert_eq!(store.total_requests(), 2);
    }
}
