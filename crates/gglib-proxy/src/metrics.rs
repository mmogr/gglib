//! In-memory metrics store for the proxy pipeline.
//!
//! [`ContextMetricsStore`] is a fixed-capacity ring buffer that records one
//! [`ContextSnapshot`] per handled `/v1/chat/completions` request. It feeds
//! the `recent_requests` field of [`crate::dashboard::DashboardSnapshot`] —
//! the unified data contract returned by `GET /v1/proxy/status` and pushed
//! over `GET /v1/proxy/status/stream`, consumed by both the CLI (`gglib
//! proxy dashboard`) and the web GUI's Proxy Dashboard modal.
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
#[derive(Debug, Clone, serde::Serialize)]
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
    /// `true` when the request pipeline originated a decode-time tool-call
    /// grammar for this request (see `request_pipeline::constrain`).
    pub grammar_enforced: bool,
    /// `true` when dialect residue — tool-call markup that survived
    /// normalization — reached this request's client-visible output (see
    /// `gglib_core::normalize::residue`). Back-patched after the response
    /// streams via [`ContextMetricsStore::flag_dialect_residue`].
    pub dialect_residue: bool,
    /// `true` when this turn's tool call failed schema validation and a
    /// re-issue with `tool_choice: "required"` produced a conformant one.
    /// Back-patched after the response streams via
    /// [`ContextMetricsStore::flag_tool_repair`].
    pub tool_repaired: bool,
    /// `true` when the pre-dispatch loop guard rejected this request with an
    /// HTTP 400 instead of forwarding it (see `loop_guard`).
    pub loop_guard_tripped: bool,
    /// Unix timestamp (seconds since epoch) at which this snapshot was recorded.
    pub recorded_at_secs: u64,
    /// Per-store sequence number assigned by [`ContextMetricsStore::record`].
    /// Identifies this snapshot for post-stream back-patching; callers pass
    /// `0` and the store overwrites it. Not part of the wire contract.
    #[serde(skip)]
    pub seq: u64,
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
    /// Count of requests whose client-visible output carried dialect
    /// residue, including flags for snapshots already evicted from the ring
    /// buffer — eviction must not lose the count.
    dialect_residue_total: AtomicU64,
    /// Count of turns whose tool call failed schema validation and was
    /// re-issued with `tool_choice: "required"`.
    ///
    /// Counted whether or not the re-issue worked. An attempt is evidence
    /// that this model's `auto` path is unconstrained — the per-model
    /// grammar-presence signal ADR 0002 left with no runtime source, readable
    /// today only from a `--verbose` llama-server log.
    tool_repairs_attempted: AtomicU64,
    /// Of those, the ones that produced a conformant call.
    ///
    /// Tracked separately because the ratio is the interesting number: a high
    /// attempt rate with a low success rate means `required` is not fixing
    /// what this model gets wrong, which is a different problem from an
    /// unconstrained `auto` path.
    tool_repairs_succeeded: AtomicU64,
    /// The process-lifetime per-model defect ledger, when one was injected.
    ///
    /// Every signal the ledger wants already passes through this store with
    /// the model name attached — `record` sees each request and the loop
    /// guard's trips, `flag_tool_repair` sees each repair — so forwarding
    /// from here reaches all of them with zero call-site changes. `None` in
    /// tests and in any embedding that has no scheduler to read it.
    ledger: Option<std::sync::Arc<gglib_core::domain::defects::ModelDefectLedger>>,
}

impl ContextMetricsStore {
    /// Create a new store with the default ring-buffer capacity
    /// ([`MAX_SNAPSHOTS`]).
    pub fn new() -> Self {
        Self {
            snapshots: Mutex::new(VecDeque::with_capacity(MAX_SNAPSHOTS)),
            total_requests: AtomicU64::new(0),
            dialect_residue_total: AtomicU64::new(0),
            tool_repairs_attempted: AtomicU64::new(0),
            tool_repairs_succeeded: AtomicU64::new(0),
            ledger: None,
        }
    }

    /// Attach the process-lifetime defect ledger; see the field docs.
    #[must_use]
    pub fn with_ledger(
        mut self,
        ledger: std::sync::Arc<gglib_core::domain::defects::ModelDefectLedger>,
    ) -> Self {
        self.ledger = Some(ledger);
        self
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
    /// Returns the snapshot's sequence number, used to back-patch
    /// stream-detected flags via [`Self::flag_dialect_residue`].
    pub fn record(&self, mut snapshot: ContextSnapshot) -> u64 {
        let seq = self.total_requests.fetch_add(1, Ordering::Relaxed);
        snapshot.seq = seq;

        if let Some(ledger) = &self.ledger {
            if snapshot.loop_guard_tripped {
                ledger.record_loop_guard_trip(&snapshot.model_name);
            } else {
                ledger.record_request(&snapshot.model_name);
            }
        }

        let mut guard = self.snapshots.lock().unwrap_or_else(|e| e.into_inner());
        guard.push_back(snapshot);
        if guard.len() > MAX_SNAPSHOTS {
            guard.pop_front();
        }
        // `guard` drops here — lock released.
        seq
    }

    /// Mark the snapshot recorded with `seq` as having leaked dialect
    /// residue into client-visible output.
    ///
    /// The total counter bumps unconditionally; the per-snapshot flag is
    /// best-effort within the ring buffer's window — a snapshot already
    /// evicted (50+ requests later) still counts, it just cannot be shown
    /// in the recent-request list.
    pub fn flag_dialect_residue(&self, seq: u64) {
        self.dialect_residue_total.fetch_add(1, Ordering::Relaxed);
        let mut guard = self.snapshots.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(snapshot) = guard.iter_mut().find(|s| s.seq == seq) {
            snapshot.dialect_residue = true;
            // Also per model. This was the one flag of the three that never
            // reached the ledger, so drift was visible fleet-wide but could
            // not be attributed — and attribution is the whole point, since
            // residue is a property of one model's dialect, not of traffic.
            if let Some(ledger) = &self.ledger {
                ledger.record_dialect_residue(&snapshot.model_name);
            }
        }
    }

    /// Total requests flagged for dialect residue, eviction-safe.
    pub fn dialect_residue_total(&self) -> u64 {
        self.dialect_residue_total.load(Ordering::Relaxed)
    }

    /// Record one tool-call repair attempt and whether it worked.
    ///
    /// Back-patches the per-snapshot flag the same best-effort way
    /// [`Self::flag_dialect_residue`] does: the totals are exact, the flag is
    /// visible only while the snapshot remains in the ring buffer.
    pub fn flag_tool_repair(&self, seq: u64, succeeded: bool) {
        self.tool_repairs_attempted.fetch_add(1, Ordering::Relaxed);
        if succeeded {
            self.tool_repairs_succeeded.fetch_add(1, Ordering::Relaxed);
        }
        let mut guard = self.snapshots.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(snapshot) = guard.iter_mut().find(|s| s.seq == seq) {
            snapshot.tool_repaired = succeeded;
            // The ring row still knows its model; a repair whose snapshot
            // was already evicted keeps the fleet counter above but is lost
            // to the per-model ledger — a bounded, bias-free undercount on
            // exactly the busiest traffic, noted in the ledger's docs.
            if let Some(ledger) = &self.ledger {
                ledger.record_repair(&snapshot.model_name, succeeded);
            }
        }
    }

    /// Count one upstream mid-stream failure against `seq`'s model.
    ///
    /// Unlike its two siblings above this keeps no fleet-wide total, because
    /// one already exists: `UpstreamHealth` counts every upstream death for
    /// the dashboard. What is missing there, and supplied here, is *which
    /// model* died — a fleet counter cannot tell a single sick model from a
    /// sick server.
    ///
    /// The ring row is consulted only for its model name, so an event whose
    /// snapshot was already evicted is lost to the per-model ledger. That is
    /// the same bounded, bias-free undercount [`Self::flag_tool_repair`]
    /// accepts, on exactly the busiest traffic.
    pub fn flag_stream_error(&self, seq: u64) {
        let guard = self.snapshots.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(snapshot) = guard.iter().find(|s| s.seq == seq)
            && let Some(ledger) = &self.ledger
        {
            ledger.record_stream_error(&snapshot.model_name);
        }
    }

    /// Count one generation cut off at the token ceiling against `seq`'s model.
    pub fn flag_truncated_generation(&self, seq: u64) {
        self.with_model(seq, |ledger, model| {
            ledger.record_truncated_generation(model);
        });
    }

    /// Count one turn that produced nothing client-renderable.
    ///
    /// `reasoning_only` distinguishes a model that stranded its whole answer
    /// in `reasoning_content` from one that produced nothing at all.
    pub fn flag_empty_response(&self, seq: u64, reasoning_only: bool) {
        self.with_model(seq, |ledger, model| {
            ledger.record_empty_response(model, reasoning_only);
        });
    }

    /// Count one turn whose tool call could not be validated at all.
    pub fn flag_unvalidatable_schema(&self, seq: u64) {
        self.with_model(seq, |ledger, model| {
            ledger.record_unvalidatable_schema(model);
        });
    }

    /// Count one turn whose normalization discarded a malformed tool call.
    pub fn flag_normalization_error(&self, seq: u64) {
        self.with_model(seq, |ledger, model| {
            ledger.record_normalization_error(model);
        });
    }

    /// Look up `seq`'s model name and hand it to the ledger.
    ///
    /// The ring row is consulted only for its model name, so an event whose
    /// snapshot was already evicted is lost to the per-model ledger — the
    /// same bounded, bias-free undercount [`Self::flag_tool_repair`] accepts,
    /// falling on exactly the busiest traffic.
    fn with_model(
        &self,
        seq: u64,
        record: impl FnOnce(&gglib_core::domain::defects::ModelDefectLedger, &str),
    ) {
        let guard = self.snapshots.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(snapshot) = guard.iter().find(|s| s.seq == seq)
            && let Some(ledger) = &self.ledger
        {
            record(ledger, &snapshot.model_name);
        }
    }

    /// Per-model defect counts, for the dashboard.
    ///
    /// The ledger is written on every request and, until this existed, read by
    /// nothing: the auto-tune scheduler was its only reader and went with ADR
    /// 0006. Counters nobody can see are not diagnosis, they are a memory
    /// leak with good intentions.
    ///
    /// Empty when no ledger is wired (the proxy can run without one).
    #[must_use]
    pub fn defect_counts(
        &self,
    ) -> std::collections::HashMap<String, gglib_core::domain::defects::ModelDefectCounts> {
        self.ledger
            .as_ref()
            .map(|ledger| ledger.snapshot())
            .unwrap_or_default()
    }

    /// Total tool-call repairs attempted, eviction-safe.
    pub fn tool_repairs_attempted(&self) -> u64 {
        self.tool_repairs_attempted.load(Ordering::Relaxed)
    }

    /// Total tool-call repairs that produced a conformant call.
    pub fn tool_repairs_succeeded(&self) -> u64 {
        self.tool_repairs_succeeded.load(Ordering::Relaxed)
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
            grammar_enforced: false,
            dialect_residue: false,
            tool_repaired: false,
            loop_guard_tripped: false,
            recorded_at_secs: 0,
            seq: 0,
        }
    }

    // ── Basic record + retrieve ───────────────────────────────────────────────

    /// The counters must be *readable*, not merely recorded.
    ///
    /// Until this existed the ledger's only reader was the auto-tune
    /// scheduler, which went with ADR 0006 — leaving every per-model counter
    /// accumulating into memory that nothing could observe. A day of traffic
    /// would have produced evidence nobody could look at.
    #[test]
    fn per_model_counts_are_readable_after_recording() {
        let ledger = std::sync::Arc::new(gglib_core::domain::defects::ModelDefectLedger::new());
        let store = ContextMetricsStore::new().with_ledger(std::sync::Arc::clone(&ledger));

        store.record(make_snapshot("qwen-27b"));
        let seq = store.recent(1)[0].seq;
        store.flag_stream_error(seq);
        store.flag_truncated_generation(seq);
        store.flag_empty_response(seq, true);

        let counts = store.defect_counts();
        let qwen = counts.get("qwen-27b").expect("the model appears by name");
        assert_eq!(qwen.stream_errors, 1);
        assert_eq!(qwen.truncated_generations, 1);
        assert_eq!(qwen.empty_responses, 1);
        assert_eq!(qwen.reasoning_only, 1, "nested inside the empty total");
    }

    #[test]
    fn defect_counts_are_empty_without_a_ledger() {
        let store = ContextMetricsStore::new();
        store.record(make_snapshot("qwen-27b"));
        assert!(store.defect_counts().is_empty());
    }

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

    // ── Dialect residue back-patching ─────────────────────────────────────────

    #[test]
    fn flag_by_seq_sets_the_snapshot_flag_and_counts() {
        let store = ContextMetricsStore::new();
        let a = store.record(make_snapshot("a"));
        let _b = store.record(make_snapshot("b"));

        store.flag_dialect_residue(a);

        let recent = store.recent(10);
        assert!(
            recent[0].dialect_residue,
            "flag lands on the right snapshot"
        );
        assert!(!recent[1].dialect_residue);
        assert_eq!(store.dialect_residue_total(), 1);
    }

    #[test]
    fn flag_after_eviction_still_counts_in_the_total() {
        let store = ContextMetricsStore::new();
        let first = store.record(make_snapshot("victim"));
        for i in 0..MAX_SNAPSHOTS {
            store.record(make_snapshot(&format!("m-{i}")));
        }
        // `first` is evicted by now; flagging must not panic and the
        // eviction-safe total must still increment.
        store.flag_dialect_residue(first);
        assert_eq!(store.dialect_residue_total(), 1);
        assert!(
            store
                .recent(MAX_SNAPSHOTS)
                .iter()
                .all(|s| !s.dialect_residue)
        );
    }

    #[test]
    fn seq_is_not_serialized() {
        let store = ContextMetricsStore::new();
        store.record(make_snapshot("a"));
        let json = serde_json::to_string(&store.recent(1)[0]).unwrap();
        assert!(json.contains("dialect_residue"));
        assert!(!json.contains("\"seq\""));
    }

    /// Attempts and successes are tracked separately because the ratio is the
    /// diagnostic: many attempts with few successes means `required` is not
    /// fixing what this model gets wrong, which is a different problem from an
    /// unconstrained `auto` path.
    #[test]
    fn repair_totals_count_attempts_and_successes_separately() {
        let store = ContextMetricsStore::new();
        let a = store.record(make_snapshot("m"));
        let b = store.record(make_snapshot("m"));

        store.flag_tool_repair(a, true);
        store.flag_tool_repair(b, false);

        assert_eq!(store.tool_repairs_attempted(), 2);
        assert_eq!(store.tool_repairs_succeeded(), 1);
    }

    /// The per-snapshot flag marks a repair that *worked*: a failed attempt
    /// forwarded the original, so that turn's output was never repaired.
    #[test]
    fn only_a_successful_repair_flags_its_snapshot() {
        let store = ContextMetricsStore::new();
        let seq = store.record(make_snapshot("m"));

        store.flag_tool_repair(seq, false);
        assert!(!store.recent(10)[0].tool_repaired);

        let seq2 = store.record(make_snapshot("m"));
        store.flag_tool_repair(seq2, true);
        assert!(store.recent(10).iter().any(|s| s.tool_repaired));
    }

    /// Eviction must not lose the totals — the same contract
    /// `dialect_residue_total` holds.
    #[test]
    fn repair_totals_survive_eviction() {
        let store = ContextMetricsStore::new();
        let seq = store.record(make_snapshot("m"));
        store.flag_tool_repair(seq, true);

        for _ in 0..MAX_SNAPSHOTS + 5 {
            store.record(make_snapshot("m"));
        }

        assert_eq!(store.tool_repairs_attempted(), 1);
        assert_eq!(store.tool_repairs_succeeded(), 1);
    }
}
