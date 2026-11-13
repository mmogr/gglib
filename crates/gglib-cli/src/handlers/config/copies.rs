//! Every gglib a user might run on this machine: each `gglib` on `PATH`, in
//! the order a shell tries them, and the desktop app.
//!
//! A developer can have three or four copies at once (Homebrew's, a Linux
//! package's, `make install`'s and the desktop app), all sharing one data
//! folder. Which one `gglib` runs is whichever comes first on `PATH`.
//! [`lines`] names them all for `gglib config paths`, as more `key = value`
//! lines: which one `gglib` runs, which are shadowed, which one is running
//! now, and each one's version.
//!
//! Another copy's version comes from its `--version`, which clap answers
//! before gglib opens its library or starts anything.
//! [`read_version`](super::copy_version::read_version) gives it a short wait and nothing on stdin; a copy that
//! hangs, fails or prints something else shows as `version unknown`. The
//! desktop app is never launched: on macOS its version is read from the
//! bundle's `Info.plist`, and on Linux and Windows from the `gglib` CLI
//! installed beside it.

use std::cell::RefCell;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use super::copy_version::version_word;

/// The names the desktop app's macOS bundle has had: `gglib`, and `GGLib GUI`
/// before it was called gglib. `scripts/uninstall.sh` holds the same list.
const MAC_APP_NAMES: &[&str] = &["gglib", "GGLib GUI"];

/// The desktop app's program on Linux, and its name in releases before.
const LINUX_APP_NAMES: &[&str] = &["gglib-gui", "gg-lib-gui"];

/// Where the desktop app is looked for, which depends on the platform.
pub(crate) enum Apps {
    /// App bundles (macOS), each read for its `CFBundleShortVersionString`.
    Bundles(Vec<PathBuf>),
    /// Program names to find on `PATH` (Linux). They are never run, since
    /// one may open a window; the version is the `gglib` beside it.
    OnPath(&'static [&'static str]),
    /// Install folders (Windows); the version is the `gglib` inside.
    Folders(Vec<PathBuf>),
}

/// What [`lines`] looks at: a `PATH`, the running binary and where the
/// desktop app may be.
pub(crate) struct Search {
    /// The `PATH` to walk, in order.
    pub(crate) path: OsString,
    /// The binary that is running, if known.
    pub(crate) running: Option<PathBuf>,
    /// The running binary's version, which it knows without being asked.
    pub(crate) running_version: String,
    /// Where the desktop app may be.
    pub(crate) apps: Apps,
}

impl Search {
    /// This machine: its `PATH`, this process and the platform's app
    /// locations.
    pub(crate) fn here() -> Self {
        Self {
            path: std::env::var_os("PATH").unwrap_or_default(),
            running: std::env::current_exe().ok(),
            running_version: gglib_build_info::SEMVER.to_owned(),
            apps: apps_here(),
        }
    }
}

/// Where the desktop app is installed on this platform. Every branch is
/// compiled on every platform, so each is checked everywhere.
fn apps_here() -> Apps {
    if cfg!(target_os = "macos") {
        let mut roots = vec![PathBuf::from("/Applications")];
        roots.extend(std::env::home_dir().map(|home| home.join("Applications")));
        let bundles = roots
            .iter()
            .flat_map(|root| {
                MAC_APP_NAMES
                    .iter()
                    .map(|name| root.join(format!("{name}.app")))
            })
            .collect();
        Apps::Bundles(bundles)
    } else if cfg!(windows) {
        let folders =
            std::env::var_os("LOCALAPPDATA").map(|local| PathBuf::from(local).join("gglib-gui"));
        Apps::Folders(folders.into_iter().collect())
    } else {
        Apps::OnPath(LINUX_APP_NAMES)
    }
}

/// The `key = value` lines for every copy [`Search`] finds.
///
/// Each `gglib` on `PATH` is a `gglib_on_path_N` line, in `PATH` order: the
/// first is the one `gglib` runs, the rest are shadowed, and the running
/// binary is marked `this one`. A running binary that is not on `PATH` gets
/// a `running_from` line. Each desktop app found is a `desktop_app_N` line.
/// With nothing found, `gglib_on_path` or `desktop_app` says `none found`.
/// A duplicate `PATH` entry, or a link to a file already listed, is listed
/// once, under the first path that reaches it.
///
/// `version_of` asks another copy its version; the running one is never
/// asked, and no file is asked twice, so the `gglib` beside the desktop app
/// that is also on `PATH` costs one wait.
pub(crate) fn lines(search: &Search, version_of: &dyn Fn(&Path) -> Option<String>) -> Vec<String> {
    let running = search.running.as_deref().map(canonical);
    let answers: RefCell<Vec<(PathBuf, Option<String>)>> = RefCell::default();
    let version = |bin: &Path| {
        let real = canonical(bin);
        if running.as_ref() == Some(&real) {
            return Some(search.running_version.clone());
        }
        let heard = answers
            .borrow()
            .iter()
            .find(|(asked, _)| *asked == real)
            .cloned();
        if let Some((_, answer)) = heard {
            return answer;
        }
        let answer = version_of(bin);
        answers.borrow_mut().push((real, answer.clone()));
        answer
    };
    let mut out = Vec::new();

    let copies = on_path(&search.path, "gglib");
    if copies.is_empty() {
        out.push("gglib_on_path = none found".to_owned());
    }
    for (index, (shown, real)) in copies.iter().enumerate() {
        let this = running.as_ref() == Some(real);
        let place = if index == 0 {
            "runs as `gglib`"
        } else {
            "shadowed"
        };
        let mut notes = vec![known(version(shown)), place.to_owned()];
        if this {
            notes.push("this one".to_owned());
        }
        out.push(format!(
            "gglib_on_path_{} = {} ({})",
            index + 1,
            shown.display(),
            notes.join(", ")
        ));
    }
    if let Some(shown) = &search.running
        && !copies
            .iter()
            .any(|(_, real)| running.as_ref() == Some(real))
    {
        out.push(format!("running_from = {}, not on PATH", shown.display()));
    }

    let apps = desktop_apps(search, &version);
    if apps.is_empty() {
        out.push("desktop_app = none found".to_owned());
    }
    for (index, (shown, app_version)) in apps.into_iter().enumerate() {
        out.push(format!(
            "desktop_app_{} = {} ({})",
            index + 1,
            shown.display(),
            known(app_version)
        ));
    }
    out
}

/// Each desktop app found, with its version if one could be read.
fn desktop_apps(
    search: &Search,
    version: &dyn Fn(&Path) -> Option<String>,
) -> Vec<(PathBuf, Option<String>)> {
    // The `gglib` CLI installed in `dir`, asked for its version.
    let beside = |dir: &Path| {
        let cli = dir.join(exe("gglib"));
        if is_executable(&cli) {
            version(&cli)
        } else {
            None
        }
    };
    match &search.apps {
        Apps::Bundles(bundles) => bundles
            .iter()
            .filter(|bundle| bundle.is_dir())
            .map(|bundle| (bundle.clone(), bundle_version(bundle)))
            .collect(),
        Apps::OnPath(names) => names
            .iter()
            .flat_map(|name| on_path(&search.path, name))
            .map(|(shown, _)| {
                let app_version = shown.parent().and_then(beside);
                (shown, app_version)
            })
            .collect(),
        Apps::Folders(folders) => folders
            .iter()
            .filter(|folder| folder.is_dir())
            .map(|folder| (folder.clone(), beside(folder)))
            .collect(),
    }
}

/// Every executable `name` on `path`, in order, once each: the path it was
/// found at, and the file it is after links are followed.
fn on_path(path: &OsStr, name: &str) -> Vec<(PathBuf, PathBuf)> {
    let mut found: Vec<(PathBuf, PathBuf)> = Vec::new();
    // An empty entry means the working directory to a shell; a listing of
    // installed copies leaves it out.
    for dir in std::env::split_paths(path).filter(|dir| !dir.as_os_str().is_empty()) {
        let candidate = dir.join(exe(name));
        if !is_executable(&candidate) {
            continue;
        }
        let real = canonical(&candidate);
        if !found.iter().any(|(_, seen)| *seen == real) {
            found.push((candidate, real));
        }
    }
    found
}

/// `name` as an executable's file name on this platform.
fn exe(name: &str) -> String {
    format!("{name}{}", std::env::consts::EXE_SUFFIX)
}

/// The file `path` names, links followed, or `path` itself when it cannot
/// be resolved.
fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Whether `path` is a file this user could run: on Unix, one with an
/// execute bit set.
fn is_executable(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        meta.is_file() && meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        meta.is_file()
    }
}

/// A version, or the words for none.
fn known(version: Option<String>) -> String {
    version.unwrap_or_else(|| "version unknown".to_owned())
}

/// The bundle's `CFBundleShortVersionString`, read from an XML
/// `Contents/Info.plist`. A binary plist, or one without the key, has none.
fn bundle_version(bundle: &Path) -> Option<String> {
    let plist = std::fs::read_to_string(bundle.join("Contents/Info.plist")).ok()?;
    let after = plist.split_once("<key>CFBundleShortVersionString</key>")?.1;
    let value = after.trim_start().strip_prefix("<string>")?;
    let (value, _) = value.split_once("</string>")?;
    version_word(value)
}

#[cfg(all(test, unix))]
#[path = "copies_tests.rs"]
mod tests;
