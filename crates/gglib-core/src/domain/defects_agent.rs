//! The agent path's half of the defect ledger.
//!
//! [`super::defects`] holds the ledger and every counter the **proxy** writes.
//! This file holds the one the **agent loop** writes, and the
//! [`AgentGuardSink`] implementation that lets `gglib-agent` reach it without
//! depending on the proxy or on this module's internals.
//!
//! Its own file for two reasons. `defects.rs` is a page of writer methods for
//! one caller, and folding a second caller's in would lose that; and the
//! rust-complexity ratchet is there to stop a file's budget being retreated
//! from, so a 282-line file carrying a stale 368-line baseline is not
//! somewhere to put twenty more lines merely because the ratchet would let it.

use super::defect_counts::LoopGuardTrip;
use super::defects::ModelDefectLedger;
use crate::ports::AgentGuardSink;

impl ModelDefectLedger {
    /// Count one guard decision taken on the agent path for `model`.
    ///
    /// `trip` is `None` when the guard ran and neither detector fired.
    ///
    /// Every decision bumps `agent_guard_scanned`, the ones that trip
    /// included, so a trip is inside its own denominator — the property
    /// [`ModelDefectLedger::record_loop_guard_trip`] keeps for the proxy path,
    /// and for the same reason it gives: a trip outside its own denominator
    /// would overstate every rate computed from these numbers. Here there is a
    /// second reason. The two paths exist to be compared, and a rate is only
    /// comparable with another rate if both were taken the same way.
    ///
    /// `agent_guard_trips` stays the sum of the two detector counts, as
    /// `loop_guard_trips` is of its two. Adding all three double-counts.
    pub fn record_agent_guard(&self, model: &str, trip: Option<LoopGuardTrip>) {
        self.with(model, |c| {
            c.agent_guard_scanned += 1;
            match trip {
                None => {}
                Some(LoopGuardTrip::Loop) => {
                    c.agent_guard_trips += 1;
                    c.agent_guard_loops += 1;
                }
                Some(LoopGuardTrip::Stagnation) => {
                    c.agent_guard_trips += 1;
                    c.agent_guard_stagnations += 1;
                }
            }
        });
    }
}

/// The ledger is the sink the in-process agent loop reports to.
///
/// Nothing is adapted: the port's one method is this ledger's one method. The
/// port exists so `gglib-agent` can reach a ledger it must not depend on, not
/// because the two shapes differ.
impl AgentGuardSink for ModelDefectLedger {
    fn record_decision(&self, model: &str, trip: Option<LoopGuardTrip>) {
        self.record_agent_guard(model, trip);
    }
}

#[cfg(test)]
#[path = "defects_agent_tests.rs"]
mod tests;
