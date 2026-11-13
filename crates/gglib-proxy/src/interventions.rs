//! What each record site did to a request, as the ledger's list of
//! interventions, in the order the work was done.
//!
//! The words are core's ([`InterventionKind::word`]); these three functions
//! map what the proxy knows at each site (a trip, a trim count, an ended
//! run) onto the kinds. The order matters: the loop guard's note is appended
//! to the body before the pipeline trims it, and a clamp refuses after the
//! note was given.

use gglib_core::domain::defects::LoopGuardTrip;
use gglib_core::domain::{Intervention, InterventionKind, Refusal};

/// What a forwarded request had done to it before it was sent on, in order:
/// a loop noted, then old messages trimmed.
#[must_use]
pub(crate) fn forwarded_interventions(
    messages_truncated: usize,
    trip: Option<LoopGuardTrip>,
) -> Vec<Intervention> {
    let mut done = Vec::new();
    if let Some(detector) = trip {
        done.push(Intervention::new(InterventionKind::NotedLoop { detector }));
    }
    if messages_truncated > 0 {
        done.push(Intervention::new(InterventionKind::Trimmed {
            messages: messages_truncated,
        }));
    }
    done
}

/// What the loop guard did to a request it answered itself: ended the run.
#[must_use]
pub(crate) fn ended_interventions(trip: LoopGuardTrip) -> Vec<Intervention> {
    vec![Intervention::new(InterventionKind::StoppedLoop {
        detector: trip,
    })]
}

/// What was done to a request the context budget refused: the loop note it
/// had already been given, when the guard tripped on it, and then the
/// refusal.
#[must_use]
pub(crate) fn clamped_interventions(trip: Option<LoopGuardTrip>) -> Vec<Intervention> {
    let mut done = Vec::new();
    if let Some(detector) = trip {
        done.push(Intervention::new(InterventionKind::NotedLoop { detector }));
    }
    done.push(Intervention::new(InterventionKind::Refused {
        reason: Refusal::LongerThanContext,
    }));
    done
}
