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

use gglib_core::domain::defects::LoopGuardTrip;

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
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
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
    /// re-issue, under `required` or gglib's own grammar, produced a conformant one.
    /// Back-patched after the response streams via
    /// [`ContextMetricsStore::flag_tool_repair`].
    pub tool_repaired: bool,
    /// The detector that made the pre-dispatch loop guard act on this request
    /// — forward it with a note, or refuse it with an HTTP 400 (see
    /// `loop_guard`) — or `None`.
    pub loop_guard_trip: Option<LoopGuardTrip>,
    /// Unix timestamp (seconds since epoch) at which this snapshot was recorded.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub recorded_at_secs: u64,
    /// Per-store sequence number assigned by [`ContextMetricsStore::record`].
    /// Identifies this snapshot for post-stream back-patching; callers pass
    /// `0` and the store overwrites it. Not part of the wire contract.
    ///
    /// `#[serde(skip)]` keeps it off the wire, and ts-rs honours that through
    /// its serde compatibility layer — so it is absent from the binding too,
    /// with no numeric override needed.
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
    /// re-issued, with `tool_choice: "required"` or under gglib's own grammar.
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
            if let Some(which) = snapshot.loop_guard_trip {
                ledger.record_loop_guard_trip(&snapshot.model_name, which);
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

    /// Record that one turn repeated a call and got an equal result back.
    ///
    /// Goes straight to the ledger by model name rather than through
    /// [`Self::with_model`]: the loop guard runs before admission, so no
    /// snapshot — and therefore no `seq` — exists yet, and a request that
    /// passes the guard is not recorded until it reaches the forwarder.
    /// Waiting for either would drop the observation on exactly the requests
    /// that carry it.
    pub fn record_identical_result_repeat(&self, model: &str) {
        if let Some(ledger) = &self.ledger {
            ledger.record_identical_result_repeat(model);
        }
    }

    /// Record that one turn repeated a batch, got a different answer, and was
    /// let through on that basis — the reading ADR 0010's kill criteria use.
    pub fn record_repeat_rescued(&self, model: &str) {
        if let Some(ledger) = &self.ledger {
            ledger.record_repeat_rescued(model);
        }
    }

    /// Record that one turn repeated a call whose results could not be
    /// compared — the reading that makes a zero above interpretable.
    pub fn record_repeat_not_evaluated(&self, model: &str) {
        if let Some(ledger) = &self.ledger {
            ledger.record_repeat_not_evaluated(model);
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
#[path = "metrics_tests.rs"]
mod tests;
