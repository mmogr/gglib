//! The rules a program is found by, as a value.
//!
//! This is the one place in the crate's program search that asks which OS
//! this is: [`Platform::host`] builds this machine's rules, and everything
//! else takes them as a value. Windows's rules are tested on every OS by
//! passing [`Platform::windows`], and Unix's by passing [`Platform::unix`].

use super::types::AttemptOutcome;
use std::ffi::OsStr;
use std::path::Path;

/// The directories executables are installed in by default, in the order
/// they are searched when `PATH` does not have the command.
pub(crate) const DEFAULT_DIRS: &[&str] = {
    #[cfg(target_os = "macos")]
    {
        &[
            "/opt/homebrew/bin", // Apple Silicon Homebrew
            "/usr/local/bin",    // Intel Homebrew / manual installs
            "/usr/bin",
            "/bin",
        ]
    }

    #[cfg(target_os = "linux")]
    {
        &["/usr/local/bin", "/usr/bin", "/bin"]
    }

    #[cfg(target_os = "windows")]
    {
        // Windows uses PATHEXT and system PATH, less reliance on hardcoded paths
        &[]
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        &["/usr/local/bin", "/usr/bin", "/bin"]
    }
};

/// The extensions tried when `PATHEXT` is not set.
const WINDOWS_EXTENSIONS: &[&str] = &[".cmd", ".exe", ".bat", ".com"];

/// How a file is known to be a program.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Runnable {
    /// Every file is taken for one.
    AnyFile,
    /// By its permission bits: one of the three execute bits is set.
    ByPermissionBits,
}

/// The rules a program is found by on one OS.
#[derive(Debug, Clone)]
pub(crate) struct Platform {
    /// The extensions a command is also looked for with, in the order tried.
    extensions: Vec<String>,
    /// What separates the entries of a `PATH`-style list.
    separator: char,
    /// How a file is known to be a program.
    runnable: Runnable,
    /// The directories searched after `PATH`.
    default_dirs: &'static [&'static str],
    /// Whether `/etc/paths` and `/etc/paths.d/*` name directories to search.
    reads_etc_paths: bool,
}

impl Platform {
    /// This machine's rules.
    pub(crate) fn host() -> Self {
        if cfg!(windows) {
            Self {
                default_dirs: DEFAULT_DIRS,
                ..Self::windows(std::env::var_os("PATHEXT").as_deref())
            }
        } else {
            Self {
                default_dirs: DEFAULT_DIRS,
                reads_etc_paths: cfg!(target_os = "macos"),
                ..Self::unix()
            }
        }
    }

    /// Unix's rules, with no default directories and no `/etc/paths`.
    pub(crate) fn unix() -> Self {
        Self {
            extensions: Vec::new(),
            separator: ':',
            runnable: Runnable::ByPermissionBits,
            default_dirs: &[],
            reads_etc_paths: false,
        }
    }

    /// Windows's rules, given the value of `PATHEXT` if it is set.
    pub(crate) fn windows(pathext: Option<&OsStr>) -> Self {
        let extensions = match pathext.and_then(OsStr::to_str) {
            Some(list) => list
                .split(';')
                .filter(|ext| !ext.is_empty())
                .map(String::from)
                .collect(),
            None => WINDOWS_EXTENSIONS
                .iter()
                .map(|ext| (*ext).to_string())
                .collect(),
        };
        Self {
            extensions,
            separator: ';',
            runnable: Runnable::AnyFile,
            default_dirs: &[],
            reads_etc_paths: false,
        }
    }

    /// The names `command` is looked for under, in the order they are tried.
    pub(crate) fn names(&self, command: &str) -> Vec<String> {
        let mut names = vec![command.to_string()];
        names.extend(self.extensions.iter().map(|ext| format!("{command}{ext}")));
        names
    }

    /// The entries of a `PATH`-style list, leaving out empty ones.
    pub(crate) fn split<'a>(&self, list: &'a str) -> impl Iterator<Item = &'a str> + 'a {
        list.split(self.separator).filter(|entry| !entry.is_empty())
    }

    /// A `PATH`-style list of `entries`.
    pub(crate) fn join<S: AsRef<str>>(&self, entries: &[S]) -> String {
        let entries: Vec<&str> = entries.iter().map(AsRef::as_ref).collect();
        entries.join(&self.separator.to_string())
    }

    /// The directories searched after `PATH`, in order.
    pub(crate) const fn default_dirs(&self) -> &'static [&'static str] {
        self.default_dirs
    }

    /// Whether `/etc/paths` and `/etc/paths.d/*` are read for directories.
    pub(crate) const fn reads_etc_paths(&self) -> bool {
        self.reads_etc_paths
    }

    /// Whether the file at `path` may be run. The caller has found it is a
    /// file.
    pub(crate) fn check_runnable(&self, path: &Path) -> AttemptOutcome {
        match self.runnable {
            Runnable::AnyFile => AttemptOutcome::Ok,
            Runnable::ByPermissionBits => permission_bits_let_it_run(path),
        }
    }
}

/// Whether a file's permission bits let it run. Only Unix has them; on any
/// other OS this rule passes every file.
fn permission_bits_let_it_run(path: &Path) -> AttemptOutcome {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        match std::fs::metadata(path) {
            Ok(metadata) if metadata.permissions().mode() & 0o111 == 0 => {
                AttemptOutcome::NotExecutable
            }
            Ok(_) => AttemptOutcome::Ok,
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                AttemptOutcome::PermissionDenied
            }
            Err(e) => AttemptOutcome::IoError(e.to_string()),
        }
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        AttemptOutcome::Ok
    }
}

#[cfg(test)]
#[path = "platform_tests.rs"]
mod tests;
