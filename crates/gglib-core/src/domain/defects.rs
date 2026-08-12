//! Per-model defect counters — the Tier C signals the closed loop steers by.
//!
//! The proxy records defect *events* (a loop-guard trip, a tool-call repair)
//! as they happen; the auto-tune scheduler reads *rates* over the traffic
//! since its last look and decides whether a model has earned a targeted
//! sweep. Writers never interpret and the reader never guesses: a trip is a
//! fact about one request, a rate is a claim about a model, and the split
//! keeps both honest.
//!
//! Counters are cumulative and process-lifetime (they live on the proxy
//! supervisor, like the agent cache metrics, so a proxy restart does not
//! zero them). Windowing is the *reader's* job: the scheduler keeps its own
//! per-model baselines and rates the delta, so two readers can window
//! differently without fighting over a reset button.
//!
//! Deliberately not persisted: a defect rate is a claim about recent traffic
//! on this build of everything, and yesterday's rate answering today's
//! question is exactly the staleness ADR 0001 warns about. The loop reacts
//! to what is happening, not to what once happened.

use std::collections::HashMap;
use std::sync::Mutex;

/// Cumulative defect counts for one model.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ModelDefectCounts {
    /// Requests the proxy forwarded (or would have, but for a guard) for
    /// this model — every rate's denominator.
    pub requests: u64,
    /// Requests the loop/stagnation guard rejected before dispatch.
    pub loop_guard_trips: u64,
    /// Turns whose tool call failed schema validation and was re-issued
    /// with `tool_choice: "required"`.
    pub repairs_attempted: u64,
    /// Of those, the re-issues that produced a conformant call.
    pub repairs_succeeded: u64,
    /// Streaming turns that died on an *upstream* mid-stream failure — an
    /// error event the model server emitted mid-generation, or the byte
    /// stream itself breaking.
    ///
    /// The catastrophic sibling of the repair signal. Both of the counters
    /// above require a model coherent enough to produce structured output:
    /// one counts verbatim repetition, the other a tool call that was
    /// attempted and malformed. A model whose sampling has collapsed
    /// produces neither — it emits output so far outside the expected shape
    /// that the model server kills the stream, and the person's turn simply
    /// fails, invisibly to every other counter here.
    ///
    /// Client disconnects are deliberately not in here: hanging up is a
    /// person's action, not a model defect.
    pub stream_errors: u64,
}

/// Process-lifetime per-model defect counters.
///
/// A synchronous mutex over a small map: every operation is a couple of
/// integer bumps under the lock, on paths that already do far heavier work.
#[derive(Debug, Default)]
pub struct ModelDefectLedger {
    counts: Mutex<HashMap<String, ModelDefectCounts>>,
}

impl ModelDefectLedger {
    /// Create an empty ledger.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Count one request for `model`.
    pub fn record_request(&self, model: &str) {
        self.with(model, |c| c.requests += 1);
    }

    /// Count one loop-guard rejection for `model`.
    ///
    /// Also counts the request itself: the guard fires *instead of* a
    /// forward, and a trip outside its own denominator would overstate
    /// every rate computed from these numbers.
    pub fn record_loop_guard_trip(&self, model: &str) {
        self.with(model, |c| {
            c.requests += 1;
            c.loop_guard_trips += 1;
        });
    }

    /// Count one tool-call repair attempt for `model`.
    pub fn record_repair(&self, model: &str, succeeded: bool) {
        self.with(model, |c| {
            c.repairs_attempted += 1;
            if succeeded {
                c.repairs_succeeded += 1;
            }
        });
    }

    /// Count one upstream mid-stream failure for `model`.
    ///
    /// Deliberately does *not* bump `requests`, unlike
    /// [`Self::record_loop_guard_trip`]. The guard fires *instead of* a
    /// forward, so it has to count its own denominator; a stream error
    /// happens after the request was forwarded and already counted. Bumping
    /// here would count the same request twice and deflate every rate.
    pub fn record_stream_error(&self, model: &str) {
        self.with(model, |c| c.stream_errors += 1);
    }

    /// The current counts for every model that has any.
    #[must_use]
    pub fn snapshot(&self) -> HashMap<String, ModelDefectCounts> {
        self.counts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    fn with(&self, model: &str, update: impl FnOnce(&mut ModelDefectCounts)) {
        let mut counts = self
            .counts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        update(counts.entry(model.to_owned()).or_default());
    }
}

/// The counts accumulated between two snapshots — what a windowing reader
/// actually rates.
#[must_use]
pub const fn delta(current: ModelDefectCounts, baseline: ModelDefectCounts) -> ModelDefectCounts {
    ModelDefectCounts {
        requests: current.requests.saturating_sub(baseline.requests),
        loop_guard_trips: current
            .loop_guard_trips
            .saturating_sub(baseline.loop_guard_trips),
        repairs_attempted: current
            .repairs_attempted
            .saturating_sub(baseline.repairs_attempted),
        repairs_succeeded: current
            .repairs_succeeded
            .saturating_sub(baseline.repairs_succeeded),
        stream_errors: current.stream_errors.saturating_sub(baseline.stream_errors),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_accumulate_per_model() {
        let ledger = ModelDefectLedger::new();
        ledger.record_request("a");
        ledger.record_request("a");
        ledger.record_loop_guard_trip("a");
        ledger.record_repair("b", true);
        ledger.record_repair("b", false);

        let snap = ledger.snapshot();
        assert_eq!(snap["a"].requests, 3); // a trip counts its own request
        assert_eq!(snap["a"].loop_guard_trips, 1);
        assert_eq!(snap["b"].repairs_attempted, 2);
        assert_eq!(snap["b"].repairs_succeeded, 1);
    }

    /// A stream error marks an already-forwarded request as having died; it
    /// must not also count a request, or the rate it feeds is deflated by
    /// its own denominator.
    #[test]
    fn a_stream_error_does_not_bump_its_own_denominator() {
        let ledger = ModelDefectLedger::new();
        ledger.record_request("a");
        ledger.record_stream_error("a");

        let snap = ledger.snapshot()["a"];
        assert_eq!(snap.requests, 1, "the turn was counted when forwarded");
        assert_eq!(snap.stream_errors, 1);
    }

    #[test]
    fn a_windowing_reader_rates_the_delta() {
        let ledger = ModelDefectLedger::new();
        for _ in 0..10 {
            ledger.record_request("a");
        }
        let baseline = ledger.snapshot()["a"];
        ledger.record_loop_guard_trip("a");
        ledger.record_request("a");

        let window = delta(ledger.snapshot()["a"], baseline);
        assert_eq!(window.requests, 2);
        assert_eq!(window.loop_guard_trips, 1);
    }
}
