//! The loop guard's step on the request path: scan, record, decide.
//!
//! [`crate::loop_guard`] owns the scan — what a replayed history means. This
//! module owns what the proxy does about it: which readings reach the
//! dashboard, and whether the request is forwarded or refused.
//!
//! The two were one block inside `chat_completions`. Separating them gives the
//! decision a return type a caller cannot ignore ([`GuardStep`]) and a seam a
//! test can reach without a running server — and leaves `server.rs`, which is
//! frozen at its size by the complexity ratchet, room to grow.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use tracing::warn;

use gglib_core::Settings;

use crate::loop_guard::{LoopGuardConfig, LoopGuardVerdict, scan_history};
use crate::metrics::{ContextMetricsStore, ContextSnapshot};
use crate::models::ErrorResponse;

/// What the guard decided about one request.
pub(crate) enum GuardStep {
    /// Forward it unchanged: nothing tripped, or the guard is switched off.
    Forward,
    /// Refuse it with this response, before any catalog/admission/model-swap
    /// cost. The snapshot for the refused request has already been recorded.
    Refuse(Response),
}

/// Run the guard over one request's replayed history.
///
/// Turn-level loop/stagnation guard (parity with the built-in agent's guards):
/// reject a conversation whose replayed history already shows a stuck loop
/// *before* paying admission — a runaway agentic client otherwise burns a
/// model swap plus a full generation per repeated turn. See [`crate::loop_guard`]
/// for the design (stateless history scan, fail-open).
///
/// Fail-open by construction: a settings snapshot that switches the guard off,
/// or an agent config with no loop threshold, returns [`GuardStep::Forward`]
/// without scanning anything.
pub(crate) fn run(
    settings: &Settings,
    body: &Bytes,
    model_name: &str,
    metrics: &ContextMetricsStore,
) -> GuardStep {
    let Some(guard_cfg) = LoopGuardConfig::from_settings(settings) else {
        return GuardStep::Forward;
    };
    let outcome = scan_history(body, &guard_cfg);

    // Diagnosis, not a decision: recorded for every scanned request,
    // whether or not the verdict below trips. A repeat under the
    // threshold is exactly the case the verdict cannot see, and it is
    // the one that says whether the repeat was stuck or productive.
    if outcome.identical_result_repeat {
        metrics.record_identical_result_repeat(model_name);
    } else if outcome.repeat_not_evaluated {
        metrics.record_repeat_not_evaluated(model_name);
    }
    // Its own `if`, not another arm of the chain above: this bit comes
    // from the detector's run-scoped outcome and those two from a
    // session-wide map, so a turn can genuinely be both — a batch that
    // repeated earlier with the same answer, and repeated just now with a
    // different one.
    if outcome.repeat_rescued {
        metrics.record_repeat_rescued(model_name);
    }

    match outcome.verdict {
        LoopGuardVerdict::Pass => GuardStep::Forward,
        tripped => {
            warn!(
                model = %model_name,
                verdict = ?tripped,
                "loop guard aborting request before dispatch"
            );
            metrics.record(ContextSnapshot {
                dialect_residue: false,
                tool_repaired: false,
                seq: 0,
                model_name: model_name.to_owned(),
                payload_chars_before: body.len(),
                payload_chars_after: body.len(),
                messages_truncated: 0,
                was_clamped: false,
                grammar_enforced: false,
                loop_guard_trip: tripped.trip(),
                recorded_at_secs: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            });
            let err = match tripped {
                LoopGuardVerdict::LoopDetected { signature } => {
                    ErrorResponse::loop_detected(&signature)
                }
                LoopGuardVerdict::StagnationDetected { count, max_steps } => {
                    ErrorResponse::stagnation_detected(count, max_steps)
                }
                LoopGuardVerdict::Pass => unreachable!("Pass is handled above"),
            };
            GuardStep::Refuse((StatusCode::BAD_REQUEST, Json(err)).into_response())
        }
    }
}

#[cfg(test)]
#[path = "loop_guard_step_tests.rs"]
mod tests;
