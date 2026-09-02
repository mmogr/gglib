//! Build/version metadata shared across gglib frontends.
#![doc = include_str!(concat!(env!("OUT_DIR"), "/README_GENERATED.md"))]

/// The `SemVer` version of the build (from Cargo).
pub const SEMVER: &str = env!("CARGO_PKG_VERSION");

/// How many hex characters of the commit id every gglib surface shows.
///
/// `build.rs` owns the cut and holds the same constant; the two agreeing is
/// what `tests::the_sha_is_always_the_advertised_width` checks.
pub const SHA_LEN: usize = 12;

/// The commit this binary was built from, abbreviated to [`SHA_LEN`].
///
/// `"unknown"` when there was no git checkout to read — a release tarball, or
/// a packager who supplied nothing.
///
/// The macOS About panel takes this as its `short_version`, which Cocoa
/// renders as `Version {version} ({short_version})` — so the commit belongs in
/// that slot and [`SEMVER`] in the other. Pass it only when [`HAS_GIT_SHA`]:
/// the constant that used to sit there fell back to [`SEMVER`], which printed
/// the version twice rather than omitting the parenthesis.
pub const GIT_SHA: &str = env!("GGLIB_GIT_SHA");

/// True if [`GIT_SHA`] is a commit id rather than the unavailable fallback.
pub const HAS_GIT_SHA: bool = is_abbreviated_sha(GIT_SHA);

/// The commit this binary was built from, with a `-dirty` marker when the tree
/// was unclean, or `"unknown"` outside a checkout.
///
/// Exists because `SemVer` cannot tell two dev builds apart: a CLI carrying new
/// daemon routes once silently used a same-version installed daemon and got an
/// opaque 405, where "the daemon is a different build — restart it" was the
/// real story. The daemon reports this from `/health`; the CLI compares it
/// against its own.
///
/// The `-dirty` marker is best-effort. A build script reruns when the commit
/// moves, not when a source file is edited, so a tree dirtied after the last
/// rebuild can still report clean. That is why this, and not [`LONG_VERSION`],
/// is where the marker lives: a stale `-dirty` in `--version` would be worse
/// than no marker at all.
pub const FINGERPRINT: &str = env!("GGLIB_FINGERPRINT");

/// The version string every gglib surface displays.
///
/// - `0.2.5 (a1b2c3d4e5f6)` in a git checkout
/// - `0.2.5` when git data is unavailable
pub const LONG_VERSION: &str = if HAS_GIT_SHA {
    VERSION_WITH_SHA
} else {
    SEMVER
};

/// Built unconditionally because `concat!` takes literals, not constants —
/// [`LONG_VERSION`] picks between this and a bare [`SEMVER`].
const VERSION_WITH_SHA: &str = concat!(env!("CARGO_PKG_VERSION"), " (", env!("GGLIB_GIT_SHA"), ")");

/// Whether `value` is a commit id rather than one of the unavailable markers.
///
/// # Why this does not check a fixed length
///
/// It used to demand exactly seven characters, and that is where the commit in
/// `gglib --version` went. The SHA then came from gix's `short_id()`, which
/// abbreviates the way git does — from the size of the object database — so
/// the prefix widened to eight once this repository passed 16384 packed
/// objects. Every build from a full clone then failed the length test and
/// printed a bare `SemVer`, which is the signal reserved for "there was no git
/// here at all". Nothing had been removed; the repository had simply grown.
///
/// `build.rs` now fixes the width, so nothing downstream needs to police it.
/// What is left for this check is telling a commit id from `"unknown"`, and
/// "not hex" does that on its own — no marker this crate emits is hex.
const fn is_abbreviated_sha(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty() {
        return false;
    }

    let mut i = 0;
    while i < bytes.len() {
        if !bytes[i].is_ascii_hexdigit() {
            return false;
        }
        i += 1;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::{
        FINGERPRINT, GIT_SHA, HAS_GIT_SHA, LONG_VERSION, SEMVER, SHA_LEN, is_abbreviated_sha,
    };

    #[test]
    fn a_commit_id_is_recognised_at_any_width() {
        // The regression, pinned. Seven passed before; eight is what gix
        // started emitting, and twelve is what this crate emits now.
        assert!(is_abbreviated_sha("a1b2c3d"));
        assert!(is_abbreviated_sha("3b4349aa"));
        assert!(is_abbreviated_sha("3b4349aa95a9"));
        assert!(is_abbreviated_sha(
            "3b4349aa95a91dfe6fed803417a228aad291bace"
        ));
    }

    #[test]
    fn upper_case_hex_is_a_commit_id() {
        assert!(is_abbreviated_sha("A1B2C3D"));
    }

    #[test]
    fn the_unavailable_marker_is_not_a_commit_id() {
        assert!(!is_abbreviated_sha("unknown"));
    }

    #[test]
    fn a_truncated_idempotent_marker_is_not_a_commit_id() {
        // vergen substitutes this literal when SOURCE_DATE_EPOCH or
        // VERGEN_IDEMPOTENT is set, which distro and Nix builds do. Cut to
        // SHA_LEN it becomes "VERGEN_IDEMP", which must not reach a user as a
        // commit. `build.rs` rejects it before truncating; this pins the
        // second line of defence.
        assert!(!is_abbreviated_sha("VERGEN_IDEMPOTENT_OUTPUT"));
        assert!(!is_abbreviated_sha(&"VERGEN_IDEMPOTENT_OUTPUT"[..SHA_LEN]));
    }

    #[test]
    fn a_non_hex_or_empty_value_is_not_a_commit_id() {
        assert!(!is_abbreviated_sha(""));
        assert!(!is_abbreviated_sha("a1b2c3g"), "g is not a hex digit");
    }

    #[test]
    fn the_sha_is_always_the_advertised_width() {
        // The validator above accepts 7, 8 and 12 by design, so it cannot be
        // what guards determinism. This is: if `build.rs` ever goes back to
        // letting git choose the width, two people building the same commit
        // get different strings and this fails.
        if HAS_GIT_SHA {
            assert_eq!(GIT_SHA.len(), SHA_LEN, "{GIT_SHA} is not {SHA_LEN} wide");
        } else {
            assert_eq!(GIT_SHA, "unknown");
        }
    }

    #[test]
    fn the_long_version_carries_the_commit_whenever_there_is_one() {
        if HAS_GIT_SHA {
            assert_eq!(LONG_VERSION, format!("{SEMVER} ({GIT_SHA})"));
        } else {
            assert_eq!(LONG_VERSION, SEMVER);
        }
    }

    #[test]
    fn the_fingerprint_names_the_same_commit_as_the_version() {
        if HAS_GIT_SHA {
            assert!(
                FINGERPRINT == GIT_SHA || FINGERPRINT == format!("{GIT_SHA}-dirty"),
                "{FINGERPRINT} and {GIT_SHA} disagree about the build"
            );
        } else {
            assert_eq!(FINGERPRINT, "unknown");
        }
    }
}
