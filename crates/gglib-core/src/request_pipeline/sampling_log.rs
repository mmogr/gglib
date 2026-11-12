//! The one `debug!` line that describes a request's whole sampling decision.
//!
//! [`apply`](super::apply) renders it once, after stage 4 has resolved the
//! request. Neither reasoning control is echoed by any readback ([ADR 0007]
//! finding 7a), so for those two this line is the only record of what was
//! sent.
//!
//! [ADR 0007]: https://github.com/mmogr/gglib/blob/main/docs/adr/0007-ask-the-server-for-template-capabilities.md

use tracing::debug;

use super::sampling::SamplingDecision;

/// Render one request's resolved sampling parameters and their provenance.
///
/// Call after the last stage that can still change `decision`, which is
/// stage 4.
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
        // 0007 finding 7a.
        reasoning_effort = ?r.reasoning_effort,
        reasoning_budget_tokens = ?r.reasoning_budget_tokens,
        from = %decision.sources.describe(&decision.layer_names),
        "sampling resolved"
    );
}

#[cfg(test)]
#[path = "sampling_log_tests.rs"]
mod sampling_log_tests;
