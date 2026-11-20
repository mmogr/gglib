//! The newest library another gglib opens, as a schema generation, for
//! [`hand_over`](crate::hand_over).
//!
//! `config paths` says it, on its `schema_generation` line, and has
//! answered without opening the library since 0.36.0. A copy older than
//! 0.38.0 is not asked: it opens nothing past generation 10, and the line is
//! only needed from 11 on. A copy from 0.38.0 on that prints no such line
//! was built before the line was added, when the generation was 11.

use gglib_core::utils::process::bounded::run_bounded;
use gglib_core::utils::process::cmd;
use std::path::Path;
use std::time::Duration;

/// How long another copy has to answer `config paths`. It asks every other
/// copy its version, each with [`VERSION_WAIT`](super::copy_version::VERSION_WAIT),
/// so one copy that hangs must not use this wait up.
pub(crate) const GENERATION_WAIT: Duration = Duration::from_secs(6);

/// The newest generation a copy older than 0.38.0 can open.
const BEFORE_0_38: i64 = 10;

/// The generation of every copy from 0.38.0 until `config paths` said it.
const BEFORE_THE_LINE: i64 = 11;

/// The newest generation the copy at `bin`, of `version`, opens: for one
/// older than 0.38.0, [`BEFORE_0_38`], which is the most it can open;
/// for any other, what its `config paths` says within `wait`. A version
/// that does not read as one, and a copy that fails, hangs or says
/// something else, has none.
pub(crate) fn read_generation(bin: &Path, version: &str, wait: Duration) -> Option<i64> {
    if major_minor(version)? < (0, 38) {
        return Some(BEFORE_0_38);
    }
    let output = run_bounded(cmd(bin).args(["config", "paths"]), wait).ok()?;
    said(
        output.status.success(),
        &String::from_utf8_lossy(&output.stdout),
    )
}

/// The generation in what a `config paths` that `succeeded` printed: its
/// `schema_generation` line, or [`BEFORE_THE_LINE`] when it has none. One
/// that failed said nothing.
fn said(succeeded: bool, stdout: &str) -> Option<i64> {
    if !succeeded {
        return None;
    }
    stdout
        .lines()
        .find_map(|line| line.strip_prefix("schema_generation = "))
        .map_or(Some(BEFORE_THE_LINE), |generation| {
            generation.trim().parse().ok()
        })
}

/// The first two numbers of a version such as `0.39.0-rc.1`.
fn major_minor(version: &str) -> Option<(u64, u64)> {
    let mut parts = version.split('.');
    Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::{BEFORE_0_38, BEFORE_THE_LINE, major_minor, read_generation, said};
    use std::path::Path;
    use std::time::Duration;

    #[test]
    fn the_line_is_read_and_its_absence_is_generation_eleven() {
        assert_eq!(
            said(true, "version = 1\nschema_generation = 14\n"),
            Some(14)
        );
        assert_eq!(said(true, "version = 1\n"), Some(BEFORE_THE_LINE));
        assert_eq!(said(true, "schema_generation = soon\n"), None);
        assert_eq!(said(false, "schema_generation = 14\n"), None);
    }

    /// The copy named here does not exist, so one that is asked has no
    /// answer and one that is not asked is seen not to be.
    #[test]
    fn only_a_copy_from_0_38_on_is_asked() {
        let nowhere = Path::new("no-such-folder").join("gglib");
        let ask = |version| read_generation(&nowhere, version, Duration::from_secs(1));

        assert_eq!(ask("0.37.9"), Some(BEFORE_0_38));
        assert_eq!(ask("0.38.0-rc.1"), None);
        assert_eq!(ask("1.0.0"), None);
        assert_eq!(ask("nightly"), None);
    }

    #[test]
    fn a_version_gives_its_first_two_numbers() {
        assert_eq!(major_minor("0.39.0-rc.1"), Some((0, 39)));
        assert_eq!(major_minor("1.2.3"), Some((1, 2)));
        assert_eq!(major_minor("nightly"), None);
    }
}
