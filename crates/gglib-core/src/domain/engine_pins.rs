//! The engine releases gglib is pinned to.
//!
//! A pin is the one upstream release of an engine that gglib installs, runs
//! and records its contracts against (`contracts/upstream/`). Moving it is a
//! deliberate change of its own, made by the routine in `CONTRIBUTING.md`'s
//! "The engine pins".
//!
//! llama.cpp's pin is here, in a crate that builds for iOS, so that code which
//! runs llama.cpp in-process reads the same release the desktop installs.
//! stable-diffusion.cpp's, `PINNED_SD_RELEASE`, is with its installer in
//! gglib-runtime.
//!
//! Scripts and workflows read and rewrite the pin as text: `scripts/llama_pin.sh`
//! prints it, and the Engine pins workflow moves it by its one line. So the
//! constant's declaration stays on one line, with its type and its quoted tag,
//! and it is the only line in this file that declares it.

/// The llama.cpp release gglib installs unless told otherwise.
///
/// # Why a pin rather than `latest`
///
/// Resolving `releases/latest` would change the inference engine underneath
/// gglib whenever upstream cut a release — silently, and differently for two
/// users who installed a day apart. Every compensation gglib applies (dialect
/// normalization, grammar origination, capability detection) is a bet about
/// what that engine does, and an unpinned engine makes those bets
/// unfalsifiable: when behaviour changes, there is no way to tell a gglib
/// regression from an upstream one.
///
/// Pinning does not stop gglib tracking upstream. It makes tracking a
/// deliberate, reviewable event: bump this constant, run the suite, ship the
/// bump as its own commit with the observed differences in the message.
/// CONTRIBUTING's "The engine pins" has the routine.
pub const PINNED_LLAMA_RELEASE: &str = "b11960";

#[cfg(test)]
mod tests {
    use super::*;

    /// The pin has to be a real tag shape; a stray `v` prefix or a bare
    /// number would 404 at install time on a user's machine, not here.
    #[test]
    fn the_pin_is_a_well_formed_build_tag() {
        let rest = PINNED_LLAMA_RELEASE
            .strip_prefix('b')
            .expect("pin must be a b-prefixed build tag");
        assert!(
            !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()),
            "pin must be b<digits>, got {PINNED_LLAMA_RELEASE}"
        );
    }
}
