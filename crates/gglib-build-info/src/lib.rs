//! Build/version metadata shared across gglib frontends.
#![doc = include_str!(concat!(env!("OUT_DIR"), "/README_GENERATED.md"))]

/// The `SemVer` version of the build (from Cargo).
pub const SEMVER: &str = env!("CARGO_PKG_VERSION");

/// The git SHA emitted by the build script.
///
/// This is expected to be a 7-character hex string when available; otherwise it
/// is set to `"unknown"`.
pub const GIT_SHA_SHORT: &str = env!("VERGEN_GIT_SHA");

/// Whether the build script reported the repo as dirty.
pub const GIT_DIRTY: bool = str_eq(env!("VERGEN_GIT_DIRTY"), "true");

/// True if the git SHA looks like a short hex hash.
pub const HAS_GIT_SHA: bool = is_short_hex(GIT_SHA_SHORT);

/// The “nice” version string used by CLI `--version` output.
///
/// Examples:
/// - `0.2.5 (a1b2c3d)`
/// - `0.2.5` (when git data is unavailable)
pub const LONG_VERSION_WITH_SHA: &str =
    concat!(env!("CARGO_PKG_VERSION"), " (", env!("VERGEN_GIT_SHA"), ")");

pub const LONG_VERSION: &str = if HAS_GIT_SHA {
    LONG_VERSION_WITH_SHA
} else {
    SEMVER
};

/// The short version string used in the Tauri/macOS About metadata.
///
/// Historically this is the commit hash; when unavailable it falls back to `SemVer`.
pub const ABOUT_SHORT_VERSION: &str = if HAS_GIT_SHA { GIT_SHA_SHORT } else { SEMVER };

const fn is_short_hex(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 7 {
        return false;
    }

    let mut i = 0;
    while i < 7 {
        let c = bytes[i];
        let is_digit = c >= b'0' && c <= b'9';
        let is_lower = c >= b'a' && c <= b'f';
        let is_upper = c >= b'A' && c <= b'F';
        if !(is_digit || is_lower || is_upper) {
            return false;
        }
        i += 1;
    }
    true
}

const fn str_eq(a: &str, b: &str) -> bool {
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    if a_bytes.len() != b_bytes.len() {
        return false;
    }

    let mut i = 0;
    while i < a_bytes.len() {
        if a_bytes[i] != b_bytes[i] {
            return false;
        }
        i += 1;
    }

    true
}
