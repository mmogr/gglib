//! Whether a resident was launched the way a request now asks.
//!
//! A context size and a projector are fixed when llama-server starts, so a
//! resident launched with another of either cannot serve the request: it is
//! recycled and the next pass launches a fresh one. That is how a changed
//! projector link takes effect, at the next request and with no restart asked
//! of anyone.

use std::path::Path;

use tracing::info;

use crate::process::admission::Resident;

/// Whether `resident` was launched with a context size or a projector other
/// than the `(context, projector)` this request resolved to.
pub(super) fn launched_differently(
    resident: &Resident,
    (context, projector): (u64, Option<&Path>),
) -> bool {
    if resident.context_size != context {
        info!(
            model_name = %resident.model_name,
            running_context = %resident.context_size,
            requested_context = %context,
            "resident model was launched with a different context — recycling"
        );
        return true;
    }
    if resident.projector.as_deref() != projector {
        info!(
            model_name = %resident.model_name,
            running_projector = ?resident.projector,
            requested_projector = ?projector,
            "resident model was launched with a different projector — recycling"
        );
        return true;
    }
    false
}

#[cfg(test)]
#[path = "resident_match_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "resident_match_admit_tests.rs"]
mod admit_tests;
