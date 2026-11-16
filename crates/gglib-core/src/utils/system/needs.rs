//! What each dependency `gglib config check-deps` lists is for, and what it
//! is called on each OS.
//!
//! One table, [`NEEDS`], so the report, its probe and the llama.cpp build's
//! own check read one answer. An OS is a value ([`HostOs`]) here, not a
//! `cfg`: every OS's rows are tested on every OS.

/// The OS a report describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostOs {
    Linux,
    MacOs,
    Windows,
}

impl HostOs {
    /// The OS named by a `std::env::consts::OS` spelling. Anything that is
    /// neither macOS nor Windows reads as Linux, as `check-deps` always has.
    #[must_use]
    pub fn from_consts(os: &str) -> Self {
        match os {
            "macos" => Self::MacOs,
            "windows" => Self::Windows,
            _ => Self::Linux,
        }
    }

    /// The OS this binary runs on.
    #[must_use]
    pub fn this() -> Self {
        Self::from_consts(std::env::consts::OS)
    }
}

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
}

/// A required row looked for under the same names on every OS.
const fn on_every_os(
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
const fn on_linux(
    name: &'static str,
    description: &'static str,
    probe: Probe,
    names: &'static [&'static str],
) -> Need {
    Need {
        name,
        purpose: Purpose::BuildGglib,
        required: true,
        description,
        probe,
        source: Source::Package,
        linux: names,
        macos: &[],
        windows: &[],
    }
}

/// The dependencies a report lists, in its order.
pub const NEEDS: &[Need] = &[
    on_every_os(
        "cargo",
        Purpose::BuildGglib,
        "Required for building Rust code",
        Probe::Word(1),
        Source::Url("https://rustup.rs"),
        &["cargo"],
    ),
    on_every_os(
        "rustc",
        Purpose::BuildGglib,
        "Rust compiler",
        Probe::Word(1),
        Source::Url("https://rustup.rs"),
        &["rustc"],
    ),
    on_every_os(
        "node",
        Purpose::BuildGglib,
        "Required for building web UI and Tauri",
        Probe::LineLessV,
        Source::Url("https://nodejs.org"),
        &["node"],
    ),
    on_every_os(
        "npm",
        Purpose::BuildGglib,
        "Node package manager",
        Probe::Line,
        Source::Url("https://nodejs.org"),
        &["npm"],
    ),
    on_every_os(
        "git",
        Purpose::BuildEngine,
        "Required for llama.cpp installation",
        Probe::Word(2),
        Source::Package,
        &["git"],
    ),
    on_every_os(
        "make",
        Purpose::BuildEngine,
        "Required for llama.cpp build",
        Probe::Word(2),
        Source::Package,
        &["make"],
    ),
    on_every_os(
        "gcc",
        Purpose::BuildEngine,
        "Required for llama.cpp compilation",
        Probe::Compiler,
        Source::Package,
        &["gcc"],
    ),
    on_every_os(
        "g++",
        Purpose::BuildEngine,
        "Required for llama.cpp compilation",
        Probe::Compiler,
        Source::Package,
        &["g++"],
    ),
    on_every_os(
        "pkg-config",
        Purpose::BuildEngine,
        "Required for building with system libraries",
        Probe::Line,
        Source::Package,
        &["pkg-config"],
    ),
    on_every_os(
        "libssl-dev",
        Purpose::BuildEngine,
        "Required for llama.cpp's HTTPS support",
        Probe::PkgConfig,
        Source::Package,
        &["openssl"],
    ),
    on_every_os(
        "cmake",
        Purpose::BuildEngine,
        "Required for llama.cpp build",
        Probe::Word(2),
        Source::Package,
        &["cmake"],
    ),
    Need {
        required: false,
        ..on_every_os(
            "python3",
            Purpose::Run,
            "Optional: enables the hf_xet download accelerator",
            Probe::Python3,
            Source::Package,
            &["python3", "python"],
        )
    },
    on_linux(
        "patchelf",
        "Required for Tauri AppImage bundling",
        Probe::Word(1),
        &["patchelf"],
    ),
    on_linux(
        "webkit2gtk-4.1",
        "Required for Tauri desktop app (WebView)",
        Probe::PkgConfig,
        &["webkit2gtk-4.1", "webkit2gtk-4.0"],
    ),
    on_linux(
        "librsvg",
        "Required for Tauri desktop app (SVG rendering)",
        Probe::PkgConfig,
        &["librsvg-2.0"],
    ),
    on_linux(
        "libappindicator-gtk3",
        "Required for Tauri system tray support",
        Probe::PkgConfig,
        &["ayatana-appindicator3-0.1", "appindicator3-0.1"],
    ),
    Need {
        required: false,
        ..on_linux(
            "gtk-layer-shell",
            "Anchors the tray panel beside the system tray on Wayland",
            Probe::PkgConfig,
            &["gtk-layer-shell-0"],
        )
    },
    on_linux(
        "libasound2-dev",
        "Required for audio support (ALSA)",
        Probe::PkgConfig,
        &["alsa"],
    ),
    Need {
        purpose: Purpose::BuildEngine,
        ..on_linux(
            "libcurl-dev",
            "Required for llama.cpp HTTP/HTTPS support",
            Probe::PkgConfig,
            &["libcurl"],
        )
    },
    on_linux(
        "libsqlite3-dev",
        "Required for database support",
        Probe::PkgConfig,
        &["sqlite3"],
    ),
    on_linux(
        "libclang-dev",
        "Required for Rust FFI bindings (bindgen)",
        Probe::LibClang,
        &["libclang"],
    ),
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
