//! The system's glibc, as a version to compare with what a download needs.

use std::fmt;
use std::sync::OnceLock;

use gglib_core::utils::process::bounded::{PROBE_WAIT, run_bounded};
use gglib_core::utils::process::cmd;

/// A glibc version: `major.minor`, as `2.43`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Glibc(pub(crate) u32, pub(crate) u32);

impl fmt::Display for Glibc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.0, self.1)
    }
}

impl Glibc {
    /// The version `text` names: what `getconf GNU_LIBC_VERSION` prints
    /// (`glibc 2.39`), a symbol version (`GLIBC_2.38`, `GLIBC_2.2.5`), or
    /// the number alone. Anything after `major.minor` is left out, and
    /// another library's version (`musl 1.2.5`) is not glibc's.
    pub(crate) fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let number = text
            .strip_prefix("glibc ")
            .or_else(|| text.strip_prefix("GLIBC_"))
            .unwrap_or(text);
        let mut parts = number.split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next()?.parse().ok()?;
        Some(Self(major, minor))
    }
}

/// This system's glibc: `None` off Linux, on a Linux with another libc, and
/// wherever it cannot be asked.
///
/// `getconf` is glibc's own program and is asked rather than the library,
/// which only `unsafe` code can call. Asked once: a glibc replaced under a
/// running gglib is not the one gglib's children load any sooner.
pub(crate) fn host_glibc() -> Option<Glibc> {
    static READ: OnceLock<Option<Glibc>> = OnceLock::new();
    *READ.get_or_init(|| {
        if !cfg!(target_os = "linux") {
            return None;
        }
        let said = run_bounded(cmd("getconf").arg("GNU_LIBC_VERSION"), PROBE_WAIT).ok()?;
        said.status
            .success()
            .then(|| Glibc::parse(&String::from_utf8_lossy(&said.stdout)))
            .flatten()
    })
}

#[cfg(test)]
mod tests {
    use super::{Glibc, host_glibc};

    #[test]
    fn a_version_is_read_from_getconf_a_symbol_version_or_a_number() {
        assert_eq!(Glibc::parse("glibc 2.39\n"), Some(Glibc(2, 39)));
        assert_eq!(Glibc::parse("GLIBC_2.38"), Some(Glibc(2, 38)));
        assert_eq!(Glibc::parse("GLIBC_2.2.5"), Some(Glibc(2, 2)));
        assert_eq!(Glibc::parse("2.43"), Some(Glibc(2, 43)));
    }

    #[test]
    fn anything_else_is_no_version() {
        for text in ["", "glibc", "musl 1.2.5-r9", "GLIBC_PRIVATE", "2", "2.x"] {
            assert_eq!(Glibc::parse(text), None, "{text:?}");
        }
    }

    /// 2.9 is older than 2.43: the parts are numbers, not text.
    #[test]
    fn versions_compare_as_numbers_and_print_as_they_are_written() {
        assert!(Glibc(2, 9) < Glibc(2, 43));
        assert!(Glibc(2, 43) < Glibc(3, 0));
        assert_eq!(Glibc(2, 43).to_string(), "2.43");
    }

    /// Linux with glibc answers; anything else has none to read.
    #[test]
    fn this_system_has_a_glibc_only_on_linux() {
        if cfg!(all(target_os = "linux", target_env = "gnu")) {
            assert!(host_glibc().is_some_and(|glibc| glibc >= Glibc(2, 17)));
        } else if !cfg!(target_os = "linux") {
            assert_eq!(host_glibc(), None);
        }
    }
}
