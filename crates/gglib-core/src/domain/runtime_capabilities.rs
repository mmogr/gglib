//! What the *engine* underneath gglib can do — the mirror of
//! [`GgufCapabilities`](super::gguf::GgufCapabilities).
//!
//! gglib has always had rich detection for what a **model** can do: GGUF
//! metadata drives capability flags, which drive launch flags and request
//! shaping. It has had none for what **llama-server** can do. The version was
//! read at install time, printed to the console, and discarded.
//!
//! That gap is why compensation accumulates. Every behaviour gglib applies to
//! work around a llama.cpp limitation — dialect normalization, grammar
//! origination, reasoning-tag stripping — is hardcoded as unconditionally on,
//! because there has never been a way to ask *is this still needed?*. Upstream
//! ships the fix, gglib keeps compensating, and nobody finds out.
//!
//! [`RuntimeCapabilities`] closes that loop: probe the binary once, record what
//! it is, and let a capability be a *question with an answer* rather than an
//! assumption baked into a call site.
//!
//! # Unknown means gglib compensates
//!
//! A version string this module cannot parse yields
//! [`RuntimeCapabilities::unknown`] — no build number, no flags — and every
//! compensation stays on. This is the same discipline
//! [`ModelContext::catalog_resolved`] applies to models: an empty capability
//! set means *nobody knows*, not *the feature is absent*, and the safe
//! response to not knowing is to keep doing the work ourselves. Deferring to
//! native behaviour on a runtime we failed to identify would trade a known
//! cost for an unknown risk.
//!
//! # Resolved once, held for the run
//!
//! A probe result is taken when a server is launched and held for that
//! process's lifetime. Nothing re-probes mid-request, and nothing switches
//! strategy mid-stream: a request that starts under one set of capabilities
//! finishes under the same set.
//!
//! This is deliberate. Hot-swapping parsing or constraint strategy partway
//! through a response — on the evidence of a residue hit, say — would make
//! failures depend on *when* within a stream the evidence arrived, which is
//! precisely the class of bug that cannot be reproduced from a recording. The
//! observation layer's job is to *log* divergence between what a runtime
//! claimed and what it delivered. Acting on that log is a deliberate change
//! to a threshold in this module, made between runs with the evidence in hand
//! — not an automatic reaction inside one.
//!
//! # Adding a capability
//!
//! 1. Add a `const MIN_BUILD_*` with a doc comment citing the upstream
//!    release, PR, or issue that establishes the build.
//! 2. Add the matching [`RuntimeFlags`] bit.
//! 3. Set it in [`RuntimeCapabilities::from_build`].
//! 4. Add a test pinning both sides of the threshold.
//!
//! Note what step 4 buys: the threshold is the claim, and the test is what
//! stops it drifting into folklore.
//!
//! [`ModelContext::catalog_resolved`]: crate::request_pipeline::ModelContext::catalog_resolved

use bitflags::bitflags;
use serde::{Deserialize, Serialize};

/// First llama.cpp build whose PEG-native chat parser handles delimited
/// ("constructed") tool-call dialects — the XML-style envelopes gglib's own
/// [`DelimitedToolCallParser`] was written for — natively.
///
/// Established by the b9656 release, which threaded OpenAI-wrapper leniency
/// through the PEG-native generator. Builds at or above this one *have* the
/// machinery.
///
/// **Having it is not the same as being able to rely on it.** Upstream issues
/// filed against exactly this path — a duplicate `</parameter>` dropping a
/// whole tool call ([#24807]), a thinking model emitting prose before
/// `<tool_call>` ([#20260]) — are failure modes gglib's parser already
/// handles. So this flag answers "does the runtime attempt this itself?", not
/// "should gglib stop attempting it?". The second question is settled by
/// measurement, and until it is, nothing gates on this flag.
///
/// [`DelimitedToolCallParser`]: crate::normalize::parsers::delimited::DelimitedToolCallParser
/// [#24807]: https://github.com/ggml-org/llama.cpp/issues/24807
/// [#20260]: https://github.com/ggml-org/llama.cpp/issues/20260
pub const MIN_BUILD_PEG_NATIVE_TOOL_CALLS: u32 = 9656;

bitflags! {
    /// Capabilities of the llama-server binary gglib is running against.
    ///
    /// Derived from the build number by [`RuntimeCapabilities::from_build`].
    /// Empty means *unknown runtime*, never *featureless runtime* — see the
    /// module docs.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
    pub struct RuntimeFlags: u32 {
        /// The runtime parses delimited tool-call dialects natively via its
        /// PEG-native chat parser. See [`MIN_BUILD_PEG_NATIVE_TOOL_CALLS`].
        const PEG_NATIVE_TOOL_CALLS = 0b0000_0001;
    }
}

/// What the llama-server binary underneath gglib is, and what it can do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeCapabilities {
    /// llama.cpp build number (the `9656` of tag `b9656`), when it could be
    /// determined.
    ///
    /// `None` for a binary whose version output this module does not
    /// recognise — a source build with a modified banner, a wrapper script, a
    /// future format. Recorded rather than guessed.
    pub build: Option<u32>,

    /// The raw version line as the binary reported it.
    ///
    /// Kept verbatim even when [`Self::build`] parsed cleanly, because it is
    /// the only artifact that lets a stored request record be re-interpreted
    /// after this module learns to read a format it could not read before.
    pub version_line: String,

    /// Capabilities derived from [`Self::build`].
    pub flags: RuntimeFlags,
}

impl RuntimeCapabilities {
    /// An unidentified runtime: no build, no flags, every compensation on.
    #[must_use]
    pub fn unknown(version_line: impl Into<String>) -> Self {
        Self {
            build: None,
            version_line: version_line.into(),
            flags: RuntimeFlags::empty(),
        }
    }

    /// Derive capabilities from a build number.
    ///
    /// The single place a build number becomes a capability set, so a
    /// threshold is stated once and every caller agrees about it.
    #[must_use]
    pub fn from_build(build: u32, version_line: impl Into<String>) -> Self {
        let mut flags = RuntimeFlags::empty();

        if build >= MIN_BUILD_PEG_NATIVE_TOOL_CALLS {
            flags |= RuntimeFlags::PEG_NATIVE_TOOL_CALLS;
        }

        Self {
            build: Some(build),
            version_line: version_line.into(),
            flags,
        }
    }

    /// Parse a llama-server version banner into capabilities, falling back to
    /// [`Self::unknown`] when no build number can be read.
    #[must_use]
    pub fn from_version_output(output: &str) -> Self {
        let line = version_line(output);
        parse_build_number(output).map_or_else(
            || Self::unknown(line.clone()),
            |build| Self::from_build(build, line.clone()),
        )
    }

    /// Whether this runtime has `flag`.
    ///
    /// Always `false` for an unidentified runtime, which is what keeps
    /// "unknown" and "absent" behaving identically at call sites without every
    /// call site having to remember the distinction.
    #[must_use]
    pub const fn has(&self, flag: RuntimeFlags) -> bool {
        self.flags.contains(flag)
    }

    /// Whether the runtime was identified at all.
    #[must_use]
    pub const fn is_identified(&self) -> bool {
        self.build.is_some()
    }
}

/// Extract the llama.cpp build number from a `--version` banner.
///
/// Reads two shapes, in order:
///
/// | Shape | Example | Source |
/// |---|---|---|
/// | `version: <n>` | `version: 9656 (a1b2c3d)` | llama-server's own banner |
/// | `b<n>` token | `llama-b10327-bin-linux` | release tag in a path or banner |
///
/// Returns `None` rather than a guess when neither appears — see the module
/// docs on why an unparsed runtime must not look like a featureless one.
#[must_use]
pub fn parse_build_number(output: &str) -> Option<u32> {
    if let Some(build) = output
        .split("version:")
        .skip(1)
        .find_map(|rest| leading_number(rest.trim_start()))
    {
        return Some(build);
    }

    output
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter_map(|token| token.strip_prefix('b'))
        .find_map(|rest| {
            (!rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()))
                .then(|| rest.parse().ok())
                .flatten()
        })
}

/// The leading run of ASCII digits in `s`, parsed.
fn leading_number(s: &str) -> Option<u32> {
    let digits: String = s.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// The most informative single line of a version banner.
///
/// llama-server prints the version first and build details after; the first
/// non-empty line is the one worth keeping. An entirely blank banner is
/// recorded as `"unknown"` so the field is never an empty string that reads
/// as "not recorded".
fn version_line(output: &str) -> String {
    output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("unknown")
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_llama_server_banner_shape() {
        assert_eq!(parse_build_number("version: 9656 (a1b2c3d)"), Some(9656));
    }

    #[test]
    fn reads_a_release_tag_shape() {
        assert_eq!(
            parse_build_number("llama-b10327-bin-linux-x64"),
            Some(10327)
        );
    }

    /// The banner shape wins: a path containing a stale tag must not override
    /// the version the binary reports about itself.
    #[test]
    fn the_banner_shape_takes_precedence_over_a_tag_token() {
        let output = "version: 9656 (a1b2c3d)\nbuilt from /opt/llama-b1234-src";
        assert_eq!(parse_build_number(output), Some(9656));
    }

    #[test]
    fn an_unreadable_banner_yields_no_build() {
        assert_eq!(
            parse_build_number("some custom fork, no version here"),
            None
        );
        assert_eq!(parse_build_number(""), None);
    }

    /// A `b` token that is not all digits is not a build tag.
    #[test]
    fn a_non_numeric_b_token_is_not_a_build() {
        assert_eq!(parse_build_number("built with backend=vulkan"), None);
    }

    /// The load-bearing default: a runtime we cannot identify claims nothing,
    /// so every compensation stays on.
    #[test]
    fn an_unidentified_runtime_claims_no_capabilities() {
        let caps = RuntimeCapabilities::from_version_output("mystery build");

        assert!(!caps.is_identified());
        assert!(!caps.has(RuntimeFlags::PEG_NATIVE_TOOL_CALLS));
        assert_eq!(caps.version_line, "mystery build");
    }

    #[test]
    fn a_build_below_the_threshold_lacks_peg_native() {
        let caps = RuntimeCapabilities::from_build(MIN_BUILD_PEG_NATIVE_TOOL_CALLS - 1, "v");
        assert!(!caps.has(RuntimeFlags::PEG_NATIVE_TOOL_CALLS));
    }

    #[test]
    fn the_threshold_build_itself_has_peg_native() {
        let caps = RuntimeCapabilities::from_build(MIN_BUILD_PEG_NATIVE_TOOL_CALLS, "v");
        assert!(caps.has(RuntimeFlags::PEG_NATIVE_TOOL_CALLS));
        assert!(caps.is_identified());
    }

    #[test]
    fn the_raw_version_line_survives_a_successful_parse() {
        let caps =
            RuntimeCapabilities::from_version_output("version: 9700 (deadbee)\nbuilt with cc");
        assert_eq!(caps.build, Some(9700));
        assert_eq!(caps.version_line, "version: 9700 (deadbee)");
    }

    #[test]
    fn a_blank_banner_records_a_placeholder_rather_than_an_empty_line() {
        assert_eq!(
            RuntimeCapabilities::from_version_output("\n  \n").version_line,
            "unknown"
        );
    }
}
