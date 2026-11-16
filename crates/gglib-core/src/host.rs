//! The machine gglib runs on: what it is, what it is called, and the shell a
//! command line gglib shows is written for.
//!
//! The one place that asks which OS this is ([`HostOs::current`]). The
//! daemon reports it on `GET /api/version` ([`Host`]), so a page names the
//! machine the service runs on, which a browser tab on another machine is
//! not.

use serde::{Deserialize, Serialize};

/// The OS of the machine gglib runs on.
///
/// Three, as gglib is built for three: any other OS reads as Linux, the
/// nearest of them (its paths, its shell, its tools).
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

    /// The OS a `std::env::consts::OS` spelling names, passed as a value so
    /// a test can name any: `"macos"`, `"windows"`, and anything else as
    /// Linux.
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

    /// What a machine of this OS is called, after "This" or "this": a
    /// surface that names the machine the service runs on says "This Mac",
    /// "This PC" or "This computer".
    #[must_use]
    pub const fn machine(self) -> &'static str {
        match self {
            Self::MacOs => "Mac",
            Self::Windows => "PC",
            Self::Linux => "computer",
        }
    }

    /// The characters that separate the names in a path on this OS.
    #[must_use]
    pub const fn separators(self) -> &'static str {
        match self {
            Self::Windows => r"\/",
            Self::MacOs | Self::Linux => "/",
        }
    }

    /// The shell a person on this OS pastes a command line into: PowerShell
    /// on Windows, which Windows Terminal, the terminal Windows 11 opens,
    /// starts in; a POSIX shell anywhere else.
    #[must_use]
    pub const fn shell(self) -> Shell {
        match self {
            Self::Windows => Shell::PowerShell,
            Self::MacOs | Self::Linux => Shell::Posix,
        }
    }
}

/// The machine the service runs on, as `GET /api/version` reports it: its
/// OS, what it is called, and how its paths are written.
///
/// A page names the machine in these words, whichever machine the page is
/// drawn on: a browser tab on a laptop reading a service on a desktop says
/// what the desktop is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Host {
    /// The OS.
    pub os: HostOs,
    /// What the machine is called ([`HostOs::machine`]): `"Mac"`, `"PC"`,
    /// `"computer"`.
    pub machine: String,
    /// The characters that separate the names in a path
    /// ([`HostOs::separators`]): `"/"`, or `"\/"` on Windows. A surface that
    /// shows a file's name from its path splits on these.
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

/// The characters a POSIX shell reads as themselves, beside letters and
/// digits.
const POSIX_PLAIN: &str = "_./:@%+=,-";

/// The characters PowerShell reads as themselves in an argument, beside
/// letters and digits, when the first is neither `-` nor `~`. `,` (a list),
/// `@` (splatting), `*` and `?` are not among them; `~` is, inside a token,
/// for a Windows short name (`RUNNER~1`), and PowerShell 7.4 expands only a
/// leading one.
const POWERSHELL_PLAIN: &str = r"_./\:%+=-~";

/// PowerShell's single quotes: the ASCII one and the four typographic ones
/// it also reads as one. Inside a single-quoted string each is doubled.
const POWERSHELL_QUOTES: [char; 5] = ['\'', '\u{2018}', '\u{2019}', '\u{201A}', '\u{201B}'];

impl Shell {
    /// `value` as one token of a line for this shell: as it is when the
    /// shell reads it as itself, otherwise in single quotes.
    ///
    /// - POSIX: plain when every character is a letter, a digit or one of
    ///   [`POSIX_PLAIN`]; quoted otherwise or when empty, a `'` written
    ///   `'\''`.
    /// - PowerShell: plain when every character is a letter, a digit or one
    ///   of [`POWERSHELL_PLAIN`] and the first is neither `-` (PowerShell
    ///   splits a bare `-x.y` into `-x` and `.y`) nor `~` (a home folder);
    ///   quoted otherwise or when empty, each quote doubled. A backslash is
    ///   no escape there, so a Windows path is left as it is. When it starts
    ///   a program, Windows PowerShell 5.1 drops an empty argument, does not
    ///   escape a `"` inside one, and does not escape the `\` that ends one
    ///   with a space (`'D:\My Cache\'` reaches the program as
    ///   `"D:\My Cache\"`, read as `D:\My Cache"` and the rest of the line
    ///   run into it); PowerShell 7.3 and later pass all three as written.
    #[must_use]
    pub fn token(self, value: &str) -> String {
        let plain_in = |extra: &str| {
            !value.is_empty()
                && value
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || extra.contains(c))
        };
        match self {
            Self::Posix if plain_in(POSIX_PLAIN) => value.to_owned(),
            Self::Posix => format!("'{}'", value.replace('\'', r"'\''")),
            Self::PowerShell if !value.starts_with(['-', '~']) && plain_in(POWERSHELL_PLAIN) => {
                value.to_owned()
            }
            Self::PowerShell => {
                let mut quoted = String::from("'");
                for c in value.chars() {
                    if POWERSHELL_QUOTES.contains(&c) {
                        quoted.push(c);
                    }
                    quoted.push(c);
                }
                quoted.push('\'');
                quoted
            }
        }
    }

    /// `program` as the first token of a line for this shell. PowerShell
    /// reads a quoted first token as a string, not a command, so a quoted
    /// program is called with `&`.
    #[must_use]
    pub fn program(self, program: &str) -> String {
        let token = self.token(program);
        match self {
            Self::PowerShell if token != program => format!("& {token}"),
            Self::Posix | Self::PowerShell => token,
        }
    }
}

#[cfg(test)]
#[path = "host_tests.rs"]
mod tests;
