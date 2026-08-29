//! Wire type for the build-identity surface.
//!
//! Every field comes from `gglib-build-info`, the one place that knows what
//! commit this binary was built from. The dashboard reads it so a bug report
//! from the GUI names a build that can actually be checked out — the same
//! thing `gglib --version` gives someone on the terminal.

use serde::Serialize;

/// What build the daemon answering this request is.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub(crate) struct VersionDto {
    /// The `SemVer` release, e.g. `"0.15.4"`.
    ///
    /// Kept separate from `display` because the agentic-eval export writes
    /// this field alone, and its CLI counterpart writes the bare version too —
    /// the two exports have to stay comparable.
    pub semver: String,
    /// The commit, abbreviated to a fixed width, or `"unknown"` outside a
    /// git checkout.
    pub sha: String,
    /// The commit plus a `-dirty` marker for an unclean tree. This is what
    /// `/health` reports and what the CLI compares against to detect a daemon
    /// left running from a different build.
    pub fingerprint: String,
    /// What a user should be shown: `"0.15.4 (a1b2c3d4e5f6)"`, or just the
    /// version when no commit is available.
    pub display: String,
}

impl VersionDto {
    /// Read the constants this binary was compiled with.
    pub(crate) fn current() -> Self {
        Self {
            semver: gglib_build_info::SEMVER.to_owned(),
            sha: gglib_build_info::GIT_SHA.to_owned(),
            fingerprint: gglib_build_info::FINGERPRINT.to_owned(),
            display: gglib_build_info::LONG_VERSION.to_owned(),
        }
    }
}
