//! Platform-specific path detection and resolution.
//!
//! The data and resource roots every other path hangs off, and the one place
//! that asks whether this binary was built from a source checkout.
//! Public API is exposed through sibling modules.

use std::env;
use std::fs;
use std::path::PathBuf;

use super::error::PathError;

/// The source checkout this binary was built from, while that checkout is
/// still there.
///
/// This says nothing about where data lives: every build keeps its state in
/// [`data_root`], whatever it was built from. It answers two narrower
/// questions a build from source has: whether `gglib config check-deps`
/// holds it to what building gglib and its engines needs, and where
/// `gglib gui` finds an app built in the same checkout.
///
/// `GGLIB_REPO_ROOT` is the workspace root of whoever compiled this binary.
/// For a release built in CI that is a runner path no user machine has, so
/// the answer there is `None`. A wrong answer is harmless: `check-deps` marks
/// the build tools needed, or `gglib gui` looks in one more place.
#[must_use]
pub fn source_checkout() -> Option<PathBuf> {
    let repo_root = PathBuf::from(env!("GGLIB_REPO_ROOT"));
    (repo_root.join("Cargo.toml").is_file() && repo_root.join(".git").exists()).then_some(repo_root)
}

/// The per-user directory every gglib build keeps its state in.
///
/// Resolution order:
/// 1. `GGLIB_DATA_DIR` environment variable
/// 2. The OS's per-user data directory: `~/.local/share/gglib` on Linux,
///    `~/Library/Application Support/gglib` on macOS,
///    `%LOCALAPPDATA%\gglib` on Windows
///
/// One folder for every build, packaged or built from a checkout, so the
/// desktop app, the CLI and the daemon always agree on the database, the
/// daemon token and lock, and the llama.cpp install, whichever of them is
/// running. A checkout holds source and build output and nothing else; to
/// keep a development build's state apart, set `GGLIB_DATA_DIR` (and
/// `GGLIB_DAEMON_PORT`, so it runs a daemon of its own).
///
/// A build made for tests never answers a test with the second. That is
/// this crate's own unit tests, and any build with its `test-utils`
/// feature, which other crates turn on from `[dev-dependencies]`. There a
/// test names its root, with `isolate_data_root` (which comes before both)
/// or `GGLIB_DATA_DIR`, and one that names none is refused before the
/// per-user folder is looked for, so no test reads or makes it. A gglib
/// program such a build produced, as the `gglib` the CLI's tests start,
/// says what it is with [`this_is_a_gglib_program`] and resolves as any
/// other build does.
/// An ordinary build has none of this.
///
/// # Panics
///
/// In a build made for tests, when a test that named no root asks. A panic
/// and not an error, which the code under test could take for "nothing is
/// installed" and the test pass on. The message names both ways to name a
/// root.
pub fn data_root() -> Result<PathBuf, PathError> {
    // 0. A test binary's own root (#955)
    #[cfg(feature = "test-utils")]
    if let Some(root) = super::test_root::root() {
        return Ok(root.to_path_buf());
    }

    if let Ok(path) = env::var("GGLIB_DATA_DIR") {
        return Ok(PathBuf::from(path));
    }

    #[cfg(any(test, feature = "test-utils"))]
    assert!(
        !refuses_the_per_user_folder(),
        "A test asked for the user's own gglib data folder. Call \
         `gglib_core::paths::isolate_data_root()` at the start of the test, or set \
         `GGLIB_DATA_DIR` for it, so it reads and writes a folder of its own."
    );

    let root = dirs::data_local_dir()
        .ok_or(PathError::NoDataDir)?
        .join("gglib");

    if !root.exists() {
        fs::create_dir_all(&root).map_err(|e| PathError::CreateFailed {
            path: root.clone(),
            reason: e.to_string(),
        })?;
    }

    Ok(root)
}

/// Whether this process said it is a gglib program.
#[cfg(any(test, feature = "test-utils"))]
static PROGRAM: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Say that this process is a gglib program, which serves the user's own
/// data folder. The first line of a `main`, and called nowhere else.
///
/// It matters only to a program a test run built: [`data_root`] refuses a
/// test the per-user folder, and tells a test from a program by this call.
/// A test never makes it, and `clippy.toml` refuses the call anywhere but
/// in a `main`. An ordinary build compiles it to nothing.
pub fn this_is_a_gglib_program() {
    #[cfg(any(test, feature = "test-utils"))]
    PROGRAM.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// Whether [`data_root`] refuses the per-user folder: in a test, which is
/// every process of a build made for tests but a gglib program.
#[cfg(any(test, feature = "test-utils"))]
fn refuses_the_per_user_folder() -> bool {
    !PROGRAM.load(std::sync::atomic::Ordering::Relaxed)
}

/// Where a data root came from, for `gglib config paths`.
#[must_use]
pub fn data_root_source() -> &'static str {
    #[cfg(feature = "test-utils")]
    if super::test_root::root().is_some() {
        return "test";
    }
    if env::var_os("GGLIB_DATA_DIR").is_some() {
        "GGLIB_DATA_DIR"
    } else {
        "default"
    }
}

/// Get the root directory for application resources (llama.cpp and
/// sd-server installs).
///
/// Resolution order:
/// 1. `GGLIB_RESOURCE_DIR` environment variable
/// 2. The data root
///
/// A test binary's own root comes first here too, as for [`data_root`], so a
/// test resolves no llama-server but one it installed there itself.
pub fn resource_root() -> Result<PathBuf, PathError> {
    // 0. A test binary's own root (#955)
    #[cfg(feature = "test-utils")]
    if let Some(root) = super::test_root::root() {
        return Ok(root.to_path_buf());
    }

    if let Ok(path) = env::var("GGLIB_RESOURCE_DIR") {
        return Ok(PathBuf::from(path));
    }

    data_root()
}

/// Normalize a user-provided path, expanding `~` and making it absolute.
pub(super) fn normalize_user_path(raw: &str) -> Result<PathBuf, PathError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(PathError::EmptyPath);
    }

    let expanded = if trimmed.starts_with("~/") || trimmed == "~" {
        let home = dirs::home_dir().ok_or(PathError::NoHomeDir)?;
        if trimmed == "~" {
            home
        } else {
            home.join(trimmed.trim_start_matches("~/"))
        }
    } else {
        PathBuf::from(trimmed)
    };

    if expanded.is_absolute() {
        Ok(expanded)
    } else {
        env::current_dir()
            .map(|cwd| cwd.join(expanded))
            .map_err(|e| PathError::CurrentDirError(e.to_string()))
    }
}

#[cfg(test)]
#[path = "platform_tests.rs"]
mod tests;
