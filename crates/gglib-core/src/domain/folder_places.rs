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
use crate::paths::{canonical, drive_root, inside_on};

/// How much of a system folder is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reach {
    /// The folder itself; a folder below it is adopted.
    Itself,
    /// The folder and everything below it.
    AndBelow,
}

use Reach::{AndBelow, Itself};

/// Linux's system folders.
///
/// - `/proc`, `/sys` and `/boot`, and all below: the kernel and live process
///   state, never a project, and some of their files never end when read.
/// - The rest, themselves only: the root, the devices (`/dev`, whose
///   `/dev/shm` is a RAM disk people build in), the folders that hold the
///   system's programs, settings and data (`/usr`, `/etc`, `/var`, ...),
///   every user's home (`/home`) and every mount (`/mnt`, `/media`, `/run`,
///   under which some systems mount a USB drive). A project below one is
///   adopted: `/usr/local/src/x`, `/usr/src/linux`, `/opt/app`, `/srv/site`,
///   `/mnt/data/x`, `/run/media/me/usb/x`, `/dev/shm/build`.
const LINUX: &[(&str, Reach)] = &[
    ("/", Itself),
    ("/bin", Itself),
    ("/boot", AndBelow),
    ("/dev", Itself),
    ("/etc", Itself),
    ("/home", Itself),
    ("/lib", Itself),
    ("/lib64", Itself),
    ("/media", Itself),
    ("/mnt", Itself),
    ("/opt", Itself),
    ("/proc", AndBelow),
    ("/run", Itself),
    ("/sbin", Itself),
    ("/srv", Itself),
    ("/sys", AndBelow),
    ("/usr", Itself),
    ("/usr/bin", Itself),
    ("/usr/include", Itself),
    ("/usr/lib", Itself),
    ("/usr/lib64", Itself),
    ("/usr/libexec", Itself),
    ("/usr/local", Itself),
    ("/usr/sbin", Itself),
    ("/usr/share", Itself),
    ("/var", Itself),
];

/// macOS's system folders.
///
/// - `/System` and `/dev`, and all below: the sealed system volume and the
///   devices, never a project.
/// - The rest, themselves only: the root, the Unix folders that hold the
///   system's programs and settings (`/usr`, `/bin`, `/etc`, which resolves
///   to `/private/etc`, ...), the folders of every app (`/Applications`,
///   `/Library`), every user's home (`/Users`) and every volume
///   (`/Volumes`). A project below one is adopted: `/usr/local/src/x`,
///   `/Volumes/Work/x`. `/private/var` is refused itself only, since every
///   temporary folder is below it.
const MACOS: &[(&str, Reach)] = &[
    ("/", Itself),
    ("/Applications", Itself),
    ("/Library", Itself),
    ("/System", AndBelow),
    ("/Users", Itself),
    ("/Volumes", Itself),
    ("/bin", Itself),
    ("/dev", AndBelow),
    ("/etc", Itself),
    ("/opt", Itself),
    ("/private", Itself),
    ("/private/etc", Itself),
    ("/private/var", Itself),
    ("/sbin", Itself),
    ("/usr", Itself),
    ("/usr/bin", Itself),
    ("/usr/lib", Itself),
    ("/usr/libexec", Itself),
    ("/usr/local", Itself),
    ("/usr/sbin", Itself),
    ("/usr/share", Itself),
    ("/var", Itself),
];

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
        self.home.as_deref().is_some_and(|home| self.is(dir, home))
    }

    /// Whether the canonical `dir` is one of this machine's system folders.
    #[must_use]
    pub fn is_system(&self, dir: &Path) -> bool {
        let refuses = |folder: &Path, reach: Reach| match reach {
            Itself => self.is(dir, folder),
            AndBelow => inside_on(self.os, dir, folder).is_some(),
        };
        let listed = |list: &[(&str, Reach)]| {
            list.iter()
                .any(|(folder, reach)| refuses(Path::new(folder), *reach))
        };
        match self.os {
            HostOs::Linux => listed(LINUX),
            HostOs::MacOs => listed(MACOS),
            HostOs::Windows => {
                let windows = self.windows_dir.as_deref();
                let drive = windows.and_then(drive_root);
                windows.is_some_and(|folder| refuses(folder, AndBelow))
                    || drive.is_some_and(|root| refuses(&root, Itself))
                    || self
                        .program_files
                        .iter()
                        .any(|folder| refuses(folder, AndBelow))
            }
        }
    }

    /// Whether `dir` and `folder` are one folder, compared as this OS does.
    fn is(&self, dir: &Path, folder: &Path) -> bool {
        inside_on(self.os, dir, folder).is_some_and(|rest| rest.as_os_str().is_empty())
    }
}

#[cfg(test)]
#[path = "folder_places_tests.rs"]
mod tests;
