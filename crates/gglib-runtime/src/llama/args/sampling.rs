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
//! [ADR 0003] measured what the flags actually did, and the answer was
//! nothing, twice over:
//!
//! - Six of the seven set a value that was already llama.cpp's default on the
//!   pinned build, so they moved nothing.
//! - The seventh loses anyway. Launched with `--temp 0.7` and sent a body
//!   carrying `temperature: 1.5`, the slot reports **1.5**. The body wins, and
//!   gglib writes a body value on every request that goes through the
//!   pipeline.
//!
//! So the flags changed the behaviour of exactly one population: someone
//! bypassing gglib and curling llama-server directly. That is not a population
//! gglib's launch path exists to configure, and serving it cost a process
//! command line that misreported what the server would actually sample with —
//! which is the surface that made #739 hard to see.
//!
//! # And they were actively harmful to observation
//!
//! [ADR 0004] finding 1: every sampler launch flag overwrites the field it
//! names in `GET /props`'s `default_generation_settings`. Since gglib set them
//! to values chosen to equal upstream's, the baseline check that guards ADR
//! 0003's deferral would have compared gglib's floor against gglib's own flag
//! and reported an agreement it could never have failed to report.
//!
//! Removing them is therefore what makes `/props` a clean read of llama.cpp's
//! own defaults, and turns ADR 0003's one-off probe into a standing
//! instrument. See [`crate::llama::args::sampling::SAMPLING_SOURCE`] for what
//! the launch banner says about it.
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
/// Adding a flag here would re-blind [`crate::llama::args::sampling`]'s
/// `/props` reader — see the module docs, and
/// `gglib_proxy::props::SAMPLER_LAUNCH_FLAGS_PASSED`, which must be flipped
/// back to `true` in the same change.
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
