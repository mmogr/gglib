//! What each dependency `gglib config check-deps` lists is for, and what it
//! is called on each OS.
//!
//! One table, [`NEEDS`], so the report, its probe and the llama.cpp build's
//! own check read one answer. An OS is a value ([`HostOs`], the one
//! `gglib_core::host` keeps) here, not a `cfg`: every OS's rows are tested
//! on every OS.

use crate::host::HostOs;

/// What a dependency is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    /// Something gglib uses as it runs.
    Run,
    /// Building gglib itself from its source code.
    BuildGglib,
    /// Building an engine (llama.cpp, stable-diffusion.cpp) from source.
    BuildEngine,
}

/// How a dependency's version is read, from each name it is looked for
/// under in turn. The first name that answers is the one reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Probe {
    /// Word `n`, counted from zero, of `<name> --version`'s first line.
    Word(usize),
    /// The first line of `<name> --version`.
    Line,
    /// The first line of `<name> --version`, less a leading `v` (`node`).
    LineLessV,
    /// The version in a compiler's `--version` banner.
    Compiler,
    /// `--version`'s word 1, when it is a Python 3.
    Python3,
    /// `pkg-config --modversion <name>`: the names are pkg-config modules.
    PkgConfig,
    /// Where libclang's shared library is (Linux): no name is asked.
    LibClang,
}

/// Where a missing dependency comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// The system's package manager, named by the package table.
    Package,
    /// An installer of its own, at this address.
    Url(&'static str),
}

/// One row of [`NEEDS`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Need {
    /// What the report calls it, and the key of its package row.
    pub name: &'static str,
    pub purpose: Purpose,
    /// Whether its purpose cannot be met without it.
    pub required: bool,
    /// What it does, in the report's words.
    pub description: &'static str,
    pub probe: Probe,
    pub source: Source,
    linux: &'static [&'static str],
    macos: &'static [&'static str],
    windows: &'static [&'static str],
}

impl Need {
    /// The names it is looked for under on `os`, in order. Empty when it is
    /// not looked for there.
    #[must_use]
    pub const fn names(&self, os: HostOs) -> &'static [&'static str] {
        match os {
            HostOs::Linux => self.linux,
            HostOs::MacOs => self.macos,
            HostOs::Windows => self.windows,
        }
    }

    /// The row, wanted but not required by its purpose.
    const fn optional(self) -> Self {
        Self {
            required: false,
            ..self
        }
    }

    /// The row, not looked for on Windows: no build there runs it.
    const fn not_on_windows(self) -> Self {
        Self {
            windows: &[],
            ..self
        }
    }

    /// The row, looked for on Linux alone.
    const fn only_on_linux(self) -> Self {
        Self {
            macos: &[],
            windows: &[],
            ..self
        }
    }

    /// The row, looked for on Windows under `names` instead.
    const fn on_windows_as(self, names: &'static [&'static str]) -> Self {
        Self {
            windows: names,
            ..self
        }
    }
}

/// A required row looked for under the same names on every OS.
const fn everywhere(
    name: &'static str,
    purpose: Purpose,
    description: &'static str,
    probe: Probe,
    source: Source,
    names: &'static [&'static str],
) -> Need {
    Need {
        name,
        purpose,
        required: true,
        description,
        probe,
        source,
        linux: names,
        macos: names,
        windows: names,
    }
}

/// A required row for building gglib, looked for on Linux alone: the
/// libraries the desktop app links there.
const fn linux_library(
    name: &'static str,
    description: &'static str,
    probe: Probe,
    names: &'static [&'static str],
) -> Need {
    everywhere(
        name,
        Purpose::BuildGglib,
        description,
        probe,
        Source::Package,
        names,
    )
    .only_on_linux()
}

const RUSTUP: Source = Source::Url("https://rustup.rs");
const NODEJS: Source = Source::Url("https://nodejs.org");

/// The dependencies a report lists, in its order within each purpose.
///
/// Windows looks for fewer: its builds use cmake's Visual Studio generator
/// rather than make, and find their libraries without pkg-config, so neither
/// they nor what pkg-config is asked about are part of one. npm there is a
/// `.cmd` script, which a program is not started as under its bare name.
pub const NEEDS: &[Need] = &[
    // Running gglib.
    everywhere(
        "python3",
        Purpose::Run,
        "Turns on the optional hf_xet download accelerator",
        Probe::Python3,
        Source::Package,
        &["python3", "python"],
    )
    .optional(),
    // Building gglib from source.
    everywhere(
        "cargo",
        Purpose::BuildGglib,
        "Builds gglib's Rust code",
        Probe::Word(1),
        RUSTUP,
        &["cargo"],
    ),
    everywhere(
        "rustc",
        Purpose::BuildGglib,
        "The Rust compiler",
        Probe::Word(1),
        RUSTUP,
        &["rustc"],
    ),
    everywhere(
        "node",
        Purpose::BuildGglib,
        "Builds the web UI and the desktop app",
        Probe::LineLessV,
        NODEJS,
        &["node"],
    ),
    everywhere(
        "npm",
        Purpose::BuildGglib,
        "Installs the web UI's packages",
        Probe::Line,
        NODEJS,
        &["npm"],
    )
    .on_windows_as(&["npm.cmd"]),
    linux_library(
        "patchelf",
        "Bundles the desktop app as an AppImage",
        Probe::Word(1),
        &["patchelf"],
    ),
    linux_library(
        "webkit2gtk-4.1",
        "The desktop app's web view (development files)",
        Probe::PkgConfig,
        &["webkit2gtk-4.1", "webkit2gtk-4.0"],
    ),
    linux_library(
        "librsvg",
        "The desktop app's SVG rendering (development files)",
        Probe::PkgConfig,
        &["librsvg-2.0"],
    ),
    linux_library(
        "libappindicator-gtk3",
        "The desktop app's tray icon (development files)",
        Probe::PkgConfig,
        &["ayatana-appindicator3-0.1", "appindicator3-0.1"],
    ),
    linux_library(
        "gtk-layer-shell",
        "Anchors the tray panel beside the system tray on Wayland",
        Probe::PkgConfig,
        &["gtk-layer-shell-0"],
    )
    .optional(),
    linux_library(
        "libasound2-dev",
        "Audio support, ALSA (development files)",
        Probe::PkgConfig,
        &["alsa"],
    ),
    linux_library(
        "libsqlite3-dev",
        "Database support (development files)",
        Probe::PkgConfig,
        &["sqlite3"],
    ),
    linux_library(
        "libclang-dev",
        "Rust FFI bindings, made by bindgen",
        Probe::LibClang,
        &["libclang"],
    ),
    // Building an engine from source.
    everywhere(
        "git",
        Purpose::BuildEngine,
        "Fetches an engine's source",
        Probe::Word(2),
        Source::Package,
        &["git"],
    ),
    everywhere(
        "cmake",
        Purpose::BuildEngine,
        "Configures an engine's build",
        Probe::Word(2),
        Source::Package,
        &["cmake"],
    ),
    everywhere(
        "make",
        Purpose::BuildEngine,
        "Runs the build cmake writes",
        Probe::Word(2),
        Source::Package,
        &["make"],
    )
    .not_on_windows(),
    // In the order a build prefers them on each OS; the llama.cpp build's
    // own check asks the same list.
    Need {
        linux: &["g++", "clang++"],
        macos: &["clang++", "g++"],
        windows: &["cl", "g++", "clang++"],
        ..everywhere(
            "C++ compiler",
            Purpose::BuildEngine,
            "Compiles an engine",
            Probe::Compiler,
            Source::Package,
            &[],
        )
    },
    everywhere(
        "pkg-config",
        Purpose::BuildEngine,
        "Finds the system libraries an engine links",
        Probe::Line,
        Source::Package,
        &["pkg-config"],
    )
    .not_on_windows(),
    everywhere(
        "libssl-dev",
        Purpose::BuildEngine,
        "llama.cpp's HTTPS support (development files)",
        Probe::PkgConfig,
        Source::Package,
        &["openssl"],
    )
    .not_on_windows(),
    everywhere(
        "libcurl-dev",
        Purpose::BuildEngine,
        "llama.cpp's HTTP support (development files)",
        Probe::PkgConfig,
        Source::Package,
        &["libcurl"],
    )
    .only_on_linux(),
];

/// The rows looked for on `os`, in the report's order.
pub fn needs_on(os: HostOs) -> impl Iterator<Item = &'static Need> {
    NEEDS.iter().filter(move |need| !need.names(os).is_empty())
}

/// The row called `name`.
#[must_use]
pub fn need(name: &str) -> Option<&'static Need> {
    NEEDS.iter().find(|need| need.name == name)
}
