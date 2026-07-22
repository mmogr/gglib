//! Consecutive-failure watchdog for the upstream llama-server.
//!
//! The `/health` fast-path check in `gglib_runtime`'s process manager catches
//! a llama-server that has *crashed or wedged hard* (its `/health` endpoint
//! stops returning `200`). It does **not** catch the subtler failure mode this
//! module targets: a server whose `/health` is still green but which has
//! degraded to the point of producing **empty responses** or never returning
//! the first token. That manifests to the client as an unexplained "empty
//! response".
//!
//! [`UpstreamHealth`] accumulates such degraded outcomes across requests. When
//! [`STRIKE_THRESHOLD`] consecutive strikes occur it raises a one-shot
//! "recycle requested" flag; the chat handler consumes that flag before the
//! next request and proactively stops the current model, forcing a fresh
//! respawn — the same cure a human applies by restarting the proxy, automated.
//!
//! ## Concurrency design
//!
//! Lock-free atomics only. Every operation is a handful of `Relaxed` atomic
//! reads/writes with no `.await`, so it is cheap on the hot path and cannot
//! hold anything across an await point.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

/// Number of *consecutive* degraded responses (empty streams or first-byte
/// timeouts) that trip a proactive recycle of the upstream model server.
///
/// Two is deliberately low: a single empty response can be a legitimate model
/// artefact, but two in a row is a strong signal the upstream has degraded and
/// is worth the cost of a recycle.
pub const STRIKE_THRESHOLD: u32 = 2;

/// Serializable, point-in-time view of the watchdog's cumulative counters.
///
/// Surfaced in the proxy dashboard so the degradation this crate guards
/// against is diagnosable at a glance instead of only in logs.
#[derive(Debug, Clone, Copy, Default, serde::Serialize)]
pub struct UpstreamHealthSnapshot {
    /// Current consecutive-strike streak (resets on any healthy response).
    pub consecutive_strikes: u32,
    /// Total empty responses observed since the proxy started.
    pub total_empty_responses: u64,
    /// Total first-byte deadline expiries since the proxy started.
    pub total_first_byte_timeouts: u64,
    /// Total proactive recycles triggered since the proxy started.
    pub total_recycles: u64,
}

/// Lock-free tracker of consecutive degraded upstream responses.
///
/// Wrap in `Arc` and share across handler tasks.
#[derive(Debug, Default)]
pub struct UpstreamHealth {
    /// Count of consecutive degraded outcomes since the last healthy one.
    consecutive_strikes: AtomicU32,
    /// One-shot flag: set when the strike threshold is reached, cleared by
    /// [`UpstreamHealth::take_recycle_request`].
    recycle_requested: AtomicBool,
    /// Cumulative counters for observability (never reset).
    total_empty_responses: AtomicU64,
    total_first_byte_timeouts: AtomicU64,
    total_recycles: AtomicU64,
}

impl UpstreamHealth {
    /// Create a tracker with a zeroed strike counter.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record the terminal outcome of a streamed response.
    ///
    /// `saw_visible_output == true` resets the strike counter (the upstream is
    /// producing output and is considered healthy); `false` counts as a
    /// strike.
    ///
    /// "Visible" is load-bearing: callers must pass
    /// [`StreamOutcome::saw_visible_output`], not "the upstream emitted some
    /// frame". A turn whose entire output arrived as `reasoning_content`
    /// renders as an empty response in clients that collapse reasoning, so
    /// counting it healthy resets this streak on every retry and the recycle
    /// threshold is never reached.
    ///
    /// [`StreamOutcome::saw_visible_output`]: crate::forward::StreamOutcome
    pub fn record_stream_outcome(&self, saw_visible_output: bool) {
        if saw_visible_output {
            self.consecutive_strikes.store(0, Ordering::Relaxed);
        } else {
            self.total_empty_responses.fetch_add(1, Ordering::Relaxed);
            self.record_strike();
        }
    }

    /// Record a first-byte deadline expiry — always a strike, since the
    /// upstream failed to begin responding at all.
    pub fn record_timeout(&self) {
        self.total_first_byte_timeouts
            .fetch_add(1, Ordering::Relaxed);
        self.record_strike();
    }

    fn record_strike(&self) {
        let strikes = self.consecutive_strikes.fetch_add(1, Ordering::Relaxed) + 1;
        if strikes >= STRIKE_THRESHOLD {
            self.recycle_requested.store(true, Ordering::Relaxed);
        }
    }

    /// Consume the recycle request, if any.
    ///
    /// Returns `true` at most once per tripped threshold. On a `true` return
    /// the strike counter is also reset, so the freshly recycled server starts
    /// with a clean slate.
    pub fn take_recycle_request(&self) -> bool {
        let requested = self.recycle_requested.swap(false, Ordering::Relaxed);
        if requested {
            self.consecutive_strikes.store(0, Ordering::Relaxed);
            self.total_recycles.fetch_add(1, Ordering::Relaxed);
        }
        requested
    }

    /// Current consecutive-strike count (for observability/tests).
    #[must_use]
    pub fn strikes(&self) -> u32 {
        self.consecutive_strikes.load(Ordering::Relaxed)
    }

    /// Serializable snapshot of the cumulative counters for the dashboard.
    #[must_use]
    pub fn snapshot(&self) -> UpstreamHealthSnapshot {
        UpstreamHealthSnapshot {
            consecutive_strikes: self.consecutive_strikes.load(Ordering::Relaxed),
            total_empty_responses: self.total_empty_responses.load(Ordering::Relaxed),
            total_first_byte_timeouts: self.total_first_byte_timeouts.load(Ordering::Relaxed),
            total_recycles: self.total_recycles.load(Ordering::Relaxed),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn healthy_outcome_keeps_strikes_zero() {
        let h = UpstreamHealth::new();
        h.record_stream_outcome(true);
        h.record_stream_outcome(true);
        assert_eq!(h.strikes(), 0);
        assert!(!h.take_recycle_request());
    }

    #[test]
    fn single_strike_does_not_trip_recycle() {
        let h = UpstreamHealth::new();
        h.record_stream_outcome(false);
        assert_eq!(h.strikes(), 1);
        assert!(!h.take_recycle_request());
    }

    #[test]
    fn two_consecutive_strikes_trip_recycle_once() {
        let h = UpstreamHealth::new();
        h.record_stream_outcome(false);
        h.record_timeout();
        assert!(h.take_recycle_request());
        // One-shot: a second consume returns false and the counter is reset.
        assert!(!h.take_recycle_request());
        assert_eq!(h.strikes(), 0);
    }

    #[test]
    fn a_healthy_outcome_resets_the_strike_streak() {
        let h = UpstreamHealth::new();
        h.record_stream_outcome(false);
        h.record_stream_outcome(true);
        h.record_stream_outcome(false);
        // Only one strike since the reset — threshold not reached.
        assert!(!h.take_recycle_request());
        assert_eq!(h.strikes(), 1);
    }

    #[test]
    fn cumulative_counters_track_events() {
        let h = UpstreamHealth::new();
        h.record_stream_outcome(false); // empty #1, strike #1
        h.record_timeout(); // timeout #1, strike #2 → recycle armed
        assert!(h.take_recycle_request()); // recycle #1
        let snap = h.snapshot();
        assert_eq!(snap.total_empty_responses, 1);
        assert_eq!(snap.total_first_byte_timeouts, 1);
        assert_eq!(snap.total_recycles, 1);
        assert_eq!(snap.consecutive_strikes, 0);
    }
}
