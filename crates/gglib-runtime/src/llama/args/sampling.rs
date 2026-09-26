//! Sampler flag resolution for llama.cpp launches — which is to say, its
//! absence.
//!
//! The sibling of [`jinja`](super::jinja), [`mtp`](super::mtp) and the rest,
//! and the only one that resolves to *no flags at all*. It exists so that
//! "gglib passes no sampler flags" is a stated decision with a reason
//! attached, rather than a gap someone fills in later because nothing said
//! not to.
//!
//! # Why nothing is emitted
//!
//! A launch flag loses to the request body ([ADR 0003] measured it), and
//! gglib writes a body value on every request that goes through the pipeline.
//! A flag gglib's own requests override would configure only someone
//! bypassing gglib and curling llama-server directly — not a population
//! gglib's launch path exists to configure — at the cost of a process command
//! line that misreports what the server samples with.
//!
//! # And a flag would blind observation
//!
//! [ADR 0004] finding 1: every sampler launch flag overwrites the field it
//! names in `GET /props`'s `default_generation_settings`, so the baseline
//! check that guards ADR 0003's deferral would compare gglib's floor against
//! gglib's own flag and report an agreement it could never fail to report.
//!
//! With no flags, nothing gglib launches with masks `/props`, and ADR 0003's
//! probe is a standing instrument. A model's own `general.sampling.*` keys
//! can still move it ([ADR 0004] finding 7); `gglib_proxy::props` says how
//! the check allows for them. See
//! [`crate::llama::args::sampling::SAMPLING_SOURCE`] for what the launch
//! banner says about it.
//!
//! [ADR 0003]: https://github.com/mmogr/gglib/blob/main/docs/adr/0003-defer-sampler-defaults-to-llama-cpp.md
//! [ADR 0004]: https://github.com/mmogr/gglib/blob/main/docs/adr/0004-observe-the-sampling-boundary.md

/// What the launch banner reports for sampling.
///
/// A launch that says nothing about sampling reads as an oversight; this makes
/// the absence explicit and points at where the decision actually happens.
pub const SAMPLING_VALUE: &str = "per-request";

/// The provenance half of the banner line.
pub const SAMPLING_SOURCE: &str = "request body, no launch flags";

/// The llama-server flags gglib emits for sampling.
///
/// Always empty. Returned as a slice rather than hardcoded at the call site so
/// that the launch path has one named place to look, and so the guard test
/// below has something to assert against.
///
/// Adding a flag here would blind the `/props` baseline check in
/// `gglib_proxy::props` (see the module docs), and
/// `gglib_proxy::props::SAMPLER_LAUNCH_FLAGS_PASSED` must be set to `true` in
/// the same change.
#[must_use]
pub const fn sampler_flags() -> &'static [&'static str] {
    &[]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The guard, and the reason this module exists as more than a comment.
    ///
    /// The invariant spans two crates: the launch path emits sampler flags (or
    /// does not), and `gglib_proxy::props` decides whether its baseline check
    /// can conclude anything (or cannot). Get them out of step in one
    /// direction and a working instrument goes dark; in the other, a blind one
    /// starts reporting agreement it is structurally incapable of withholding.
    ///
    /// `gglib-runtime` depends on `gglib-proxy`, so the pairing can be
    /// asserted directly rather than left to a grep guard or a pair of
    /// comments hoping to be read together.
    #[test]
    fn no_sampler_flag_may_reappear_unnoticed() {
        assert_eq!(
            !sampler_flags().is_empty(),
            gglib_proxy::props::SAMPLER_LAUNCH_FLAGS_PASSED,
            "the launch path emits {:?}, but \
             gglib_proxy::props::SAMPLER_LAUNCH_FLAGS_PASSED says {}. These must agree. \
             A sampler launch flag overwrites the field it names in /props, so while one \
             is passed the baseline check is reading gglib's own value back and must \
             report Indeterminate rather than Matches (ADR 0004 finding 1).",
            sampler_flags(),
            gglib_proxy::props::SAMPLER_LAUNCH_FLAGS_PASSED,
        );
    }

    /// The banner must say something rather than omitting the row — an absent
    /// line is indistinguishable from a launch surface that forgot.
    #[test]
    fn the_banner_states_the_absence_rather_than_omitting_it() {
        assert!(!SAMPLING_VALUE.is_empty());
        assert!(SAMPLING_SOURCE.contains("no launch flags"));
    }
}
