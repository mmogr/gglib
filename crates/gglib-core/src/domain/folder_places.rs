//! The places a chat never takes as its folder without being told to: this
//! machine's home folder and its system folders, per OS.
//!
//! A system folder holds the operating system, its programs or every user's
//! files. A terminal opens in one more often than people think: on Windows
//! one started from the Start menu, from Win+X or as administrator starts in
//! `C:\Windows\System32`. The lists err towards adopting: a folder people
//! really keep projects in (`/usr/local/src/x`, `/opt/app`, `D:\`, a mapped
//! drive) is adopted, and most entries refuse the folder itself and not
//! what is below it.
//!
//! The machine's own places (home, the Windows directory, Program Files)
//! are values read once from the environment ([`Places::current`]), and the
//! OS is a [`HostOs`], so the rule is a pure function of a path and a
//! [`Places`] that runs the same on every OS. Paths are compared through
//! [`crate::paths::inside_on`]: canonical, without case on Windows and macOS.

use std::path::{Path, PathBuf};

use crate::host::HostOs;
use crate::paths::canonical;

/// The places on one machine a launched chat's folder is checked against.
///
/// They are its OS, its home folder and, on Windows, the folders the
/// environment names. Each path is canonical, as [`Places::resolved`] makes
/// it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Places {
    /// The OS, which picks the list and how paths are compared.
    pub os: HostOs,
    /// This user's home folder.
    pub home: Option<PathBuf>,
    /// The Windows directory, `%SystemRoot%` (else `%windir%`), refused with
    /// all below it, and its drive's root, refused itself. Read on Windows
    /// only.
    pub windows_dir: Option<PathBuf>,
    /// `%ProgramFiles%`, `%ProgramFiles(x86)%` and `%ProgramW6432%`, where
    /// installers put programs, each refused with all below it. Read on
    /// Windows only.
    pub program_files: Vec<PathBuf>,
}

impl Places {
    /// This machine's places, from the environment, canonical.
    #[must_use]
    pub fn current() -> Self {
        let var = |name: &str| {
            std::env::var_os(name)
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        };
        Self {
            os: HostOs::current(),
            home: dirs::home_dir(),
            windows_dir: var("SystemRoot").or_else(|| var("windir")),
            program_files: ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"]
                .into_iter()
                .filter_map(var)
                .collect(),
        }
        .resolved()
    }

    /// These places with each path canonical, so a home reached through a
    /// link or spelled with `..` is still home; a path that cannot be
    /// resolved is kept as given.
    #[must_use]
    pub fn resolved(self) -> Self {
        let resolved = |path: PathBuf| canonical(&path).unwrap_or(path);
        Self {
            home: self.home.map(resolved),
            windows_dir: self.windows_dir.map(resolved),
            program_files: self.program_files.into_iter().map(resolved).collect(),
            ..self
        }
    }

    /// Whether the canonical `dir` is this user's home folder.
    #[must_use]
    pub fn is_home(&self, dir: &Path) -> bool {
        self.home.as_deref() == Some(dir)
    }

    /// Whether the canonical `dir` is one of this machine's system folders.
    #[must_use]
    pub fn is_system(&self, dir: &Path) -> bool {
        dir.parent().is_none()
    }
}

#[cfg(test)]
#[path = "folder_places_tests.rs"]
mod tests;
