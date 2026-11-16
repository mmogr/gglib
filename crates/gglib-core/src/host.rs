//! The machine gglib runs on: what it is, what it is called, and the shell a
//! command line gglib shows is written for.
//!
//! Scaffolding: the shape the tests are written against, with today's
//! behaviour (every machine a Mac, every path split on `/`, every line
//! quoted for a POSIX shell).

use serde::{Deserialize, Serialize};

/// The OS of the machine gglib runs on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub enum HostOs {
    /// macOS.
    #[serde(rename = "macos")]
    MacOs,
    /// Windows.
    #[serde(rename = "windows")]
    Windows,
    /// Linux, and any other OS.
    #[serde(rename = "linux")]
    Linux,
}

impl HostOs {
    /// Every OS, in declaration order.
    pub const ALL: [Self; 3] = [Self::MacOs, Self::Windows, Self::Linux];

    /// The OS a `std::env::consts::OS` spelling names.
    #[must_use]
    pub fn from_consts(os: &str) -> Self {
        match os {
            "macos" => Self::MacOs,
            "windows" => Self::Windows,
            _ => Self::Linux,
        }
    }

    /// The OS this build runs on.
    #[must_use]
    pub fn current() -> Self {
        Self::from_consts(std::env::consts::OS)
    }

    /// What a machine of this OS is called: today, a Mac.
    #[must_use]
    pub const fn machine(self) -> &'static str {
        "Mac"
    }

    /// The characters that separate the names in a path: today, `/`.
    #[must_use]
    pub const fn separators(self) -> &'static str {
        "/"
    }

    /// The shell a person on this OS pastes a command line into.
    #[must_use]
    pub const fn shell(self) -> Shell {
        match self {
            Self::Windows => Shell::PowerShell,
            Self::MacOs | Self::Linux => Shell::Posix,
        }
    }
}

/// The machine the service runs on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Host {
    /// The OS.
    pub os: HostOs,
    /// What the machine is called.
    pub machine: String,
    /// The characters that separate the names in a path.
    pub separators: String,
}

impl Host {
    /// The machine this build runs on.
    #[must_use]
    pub fn current() -> Self {
        Self::of(HostOs::current())
    }

    /// A machine of `os`.
    #[must_use]
    pub fn of(os: HostOs) -> Self {
        Self {
            os,
            machine: os.machine().to_owned(),
            separators: os.separators().to_owned(),
        }
    }
}

/// The shell a command line is written for: the one a person on that
/// machine pastes it into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub enum Shell {
    /// A POSIX shell (`sh`, `bash`, `zsh`): Linux and macOS.
    #[serde(rename = "posix")]
    Posix,
    /// PowerShell: what Windows Terminal opens on Windows 11.
    #[serde(rename = "powershell")]
    PowerShell,
}

#[cfg(test)]
#[path = "host_tests.rs"]
mod tests;
