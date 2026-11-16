//! Another gglib's version, from its `--version`, for
//! [`copies`](super::copies).
//!
//! Every release, from 0.1.0 on, parses its command line before it opens
//! its library or starts anything, so clap answers `--version` first and
//! asking is safe. A copy that hangs is
//! killed when its wait is up; one that fails, or prints something that is
//! not a version, has none.

use gglib_core::utils::process::bounded::run_bounded;
use gglib_core::utils::process::cmd;
use std::path::Path;
use std::time::Duration;

/// How long another copy has to answer `--version`.
pub(crate) const VERSION_WAIT: Duration = Duration::from_secs(2);

/// Run `bin --version`, with nothing on stdin, and give it `wait` to answer.
/// A copy that does not start, exits with an error, says nothing that reads
/// as a version, or is still running when `wait` is up (it is then killed)
/// has no version.
pub(crate) fn read_version(bin: &Path, wait: Duration) -> Option<String> {
    let output = run_bounded(cmd(bin).arg("--version"), wait).ok()?;
    if !output.status.success() {
        return None;
    }
    version_word(String::from_utf8_lossy(&output.stdout).lines().next()?)
}

/// The version in a line such as `gglib 0.35.0 (a1b2c3d4e5f6)`: its first
/// word that starts with a digit, if that word is short and plain.
pub(super) fn version_word(line: &str) -> Option<String> {
    let word = line
        .split_whitespace()
        .find(|word| word.starts_with(|c: char| c.is_ascii_digit()))?;
    let plain = word.len() <= 40
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+' | '_'));
    plain.then(|| word.to_owned())
}
