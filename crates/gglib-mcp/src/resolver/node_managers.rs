//! Where Node's version managers keep `node`, `npm` and `npx`, on each OS.
//!
//! [`super::platform::Platform`] says which OS's managers to look for; the
//! environment is passed in, so each OS's places are tested on every OS.

use super::env::EnvProvider;
use std::path::{Path, PathBuf};

/// Which OS's version managers are looked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NodeManagers {
    /// asdf, volta and nvm, under `HOME`.
    Unix,
    /// volta, nvm-windows and fnm, where each keeps its programs on Windows.
    Windows,
}

/// A place a version manager keeps Node's programs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NodeDir {
    /// A folder holding the programs: a manager's shims, or a link to the
    /// version in use.
    Holding(PathBuf),
    /// A folder of installed versions, one folder each, with the programs
    /// in `<version>/<bin>`, or in the version's folder itself when `bin`
    /// is `None`. The version named in the file `default`, when there is
    /// one, is searched first, then each from the newest.
    Versions {
        root: PathBuf,
        bin: Option<&'static str>,
        default: Option<PathBuf>,
    },
}

impl NodeManagers {
    /// The places to look, in order, in `env`.
    pub(crate) fn dirs(self, env: &dyn EnvProvider) -> Vec<NodeDir> {
        match self {
            Self::Unix => unix(env),
            Self::Windows => windows(env),
        }
    }
}

/// asdf's shims first (a shim takes precedence), then volta's, then nvm's
/// versions, all under `HOME`; nothing without it.
fn unix(env: &dyn EnvProvider) -> Vec<NodeDir> {
    let Some(home) = var(env, "HOME") else {
        return Vec::new();
    };
    vec![
        NodeDir::Holding(home.join(".asdf/shims")),
        NodeDir::Holding(home.join(".volta/bin")),
        NodeDir::Versions {
            root: home.join(".nvm/versions/node"),
            bin: Some("bin"),
            default: Some(home.join(".nvm/alias/default")),
        },
    ]
}

/// volta's shims first (a shim takes precedence), then the version in use
/// that nvm-windows and fnm link to, then fnm's default version, then
/// nvm-windows' versions. A place whose variable is unset, and has no
/// default, is left out.
///
/// - volta: its Windows installer puts the `node`, `npm` and `npx` shims
///   in `%ProgramFiles%\Volta`, beside `volta.exe`; the shims of the
///   packages installed with it are in `%VOLTA_HOME%\bin`, and
///   `VOLTA_HOME` is `%LOCALAPPDATA%\Volta` by default (volta's layout:
///   `bin` is its `shim_dir`; its default home is `dirs::data_local_dir()`
///   + `Volta`). All three are searched.
/// - nvm-windows: `%NVM_SYMLINK%` links to the version in use; each
///   version is a folder `v<version>` under `%NVM_HOME%` holding
///   `node.exe` (nvm-windows' `nvm.go`), and its installer puts `NVM_HOME`
///   at `%APPDATA%\nvm`.
/// - fnm: a shell's version is `%FNM_MULTISHELL_PATH%` itself on Windows,
///   not its `bin` folder (fnm's `env.rs`); the default version is
///   `aliases\default` under `%FNM_DIR%`, which is `%APPDATA%\fnm` by
///   default (fnm's `config.rs` and `directories.rs`).
fn windows(env: &dyn EnvProvider) -> Vec<NodeDir> {
    let appdata = var(env, "APPDATA");
    let under = |key: &str, base: Option<&Path>, default: &str| {
        var(env, key).or_else(|| base.map(|base| base.join(default)))
    };
    let volta_home = var(env, "VOLTA_HOME");
    let volta_default = var(env, "LOCALAPPDATA").map(|local| local.join("Volta"));
    let volta_installed = var(env, "ProgramFiles").map(|programs| programs.join("Volta"));
    let fnm = under("FNM_DIR", appdata.as_deref(), "fnm");
    let nvm = under("NVM_HOME", appdata.as_deref(), "nvm");

    let mut dirs = Vec::new();
    dirs.extend(volta_home.map(|home| NodeDir::Holding(home.join("bin"))));
    dirs.extend(volta_default.map(|home| NodeDir::Holding(home.join("bin"))));
    dirs.extend(volta_installed.map(NodeDir::Holding));
    dirs.extend(var(env, "NVM_SYMLINK").map(NodeDir::Holding));
    dirs.extend(var(env, "FNM_MULTISHELL_PATH").map(NodeDir::Holding));
    dirs.extend(fnm.map(|fnm| NodeDir::Holding(fnm.join("aliases").join("default"))));
    dirs.extend(nvm.map(|root| NodeDir::Versions {
        root,
        bin: None,
        default: None,
    }));
    dirs
}

/// `key`'s value as a path, unless it is unset or empty.
fn var(env: &dyn EnvProvider, key: &str) -> Option<PathBuf> {
    env.get(key)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}
