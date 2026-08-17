//! The one `debug!` line that describes a request's whole sampling decision.
//!
//! # Why it is not inside [`resolve_sampling`](super::sampling::resolve_sampling)
//!
//! It was, and that made it wrong on exactly the model this arc exists for.
//! `resolve_sampling` is stage 4; [`effort_gate`](super::effort_gate) is stage
//! 5b, and it *deletes* a resolved `reasoning_effort` when the model's observed
//! template does not read the variable. A line rendered inside stage 4 therefore
//! printed
//!
//! ```text
//! reasoning_effort=Some(High) … from=… reasoning_effort=profile …
//! ```
//!
//! for a value that stage 5b was about to throw away — and since neither
//! reasoning control is echoed by any readback ([ADR 0007] finding 7a), that log
//! line **is** the record. An operator grepping `sampling resolved` on a
//! suppressing model would have found gglib stating, in its only surviving
//! account of the request, that it sent a level it did not send.
//!
//! The alternative was to leave the stage-4 line alone and make stage 5b's own
//! line loud enough to correct it. That was rejected: the misleading line fires
//! on *every* request while the correction fires only on a suppression, so the
//! reader has to know to go looking for a second line before they can trust the
//! first. A record that is only true when read alongside another record is not a
//! record. Rendering once, after every stage that can still change the answer,
//! costs one function call and makes the common line honest by construction.
//!
//! Stage 5b keeps its own `debug!` for what this line structurally cannot say:
//! after suppression `resolved.reasoning_effort` is `None` and its provenance
//! reads `suppressed-by-template`, so **which** level was dropped and **which**
//! rung asked for it exist nowhere else.
//!
//! [ADR 0007]: https://github.com/mmogr/gglib/blob/main/docs/adr/0007-ask-the-server-for-template-capabilities.md

use tracing::debug;

use super::sampling::SamplingDecision;

/// Render one request's resolved sampling parameters and their provenance.
///
/// Call after the last stage that can still change `decision` — today that is
/// stage 5b. See the module docs for why the position is load-bearing.
///
/// Guarded on the level being enabled because both `layer_names` and
/// [`FieldSources::describe`](crate::domain::FieldSources::describe) allocate,
/// and this sits on the busiest path in the system.
pub(super) fn log_resolution(decision: &SamplingDecision) {
    if !tracing::enabled!(tracing::Level::DEBUG) {
        return;
    }
    let r = &decision.resolved;
    debug!(
        temperature = ?r.temperature,
        top_p = ?r.top_p,
        top_k = ?r.top_k,
        max_tokens = ?r.max_tokens,
        presence_penalty = ?r.presence_penalty,
        repeat_penalty = ?r.repeat_penalty,
        min_p = ?r.min_p,
        frequency_penalty = ?r.frequency_penalty,
        dynatemp_range = ?r.dynatemp_range,
        dynatemp_exponent = ?r.dynatemp_exponent,
        top_n_sigma = ?r.top_n_sigma,
        dry_multiplier = ?r.dry_multiplier,
        dry_base = ?r.dry_base,
        dry_allowed_length = ?r.dry_allowed_length,
        dry_penalty_last_n = ?r.dry_penalty_last_n,
        // Logged like the rest, and load-bearing in a way the rest are not: no
        // readback will ever echo either of these, so this line and the
        // provenance record are the only evidence of what was resolved. See ADR
        // 0007 finding 7a — and the module docs for why that makes the position
        // of this call part of the contract.
        reasoning_effort = ?r.reasoning_effort,
        reasoning_budget_tokens = ?r.reasoning_budget_tokens,
        from = %decision.sources.describe(&decision.layer_names),
        // Which class floor was used. `sources` says a value came from "floor"
        // but not which one, and the explain surfaces cannot show this at all —
        // they resolve stored configuration with no request in hand.
        floor = decision.floor.label(),
        // Reported separately from `from`, which still names the rung the
        // temperature *would* have come from. The ceiling does not replace that
        // rung, it caps what it supplied.
        agentic_turn = decision.agentic_turn,
        agentic_ceiling = decision.agentic_ceiling_applied,
        "sampling resolved"
    );
}

#[cfg(test)]
#[path = "sampling_log_tests.rs"]
mod sampling_log_tests;
