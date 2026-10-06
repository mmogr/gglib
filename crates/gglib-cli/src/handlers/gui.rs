//! GUI launch handler.
//!
//! Handles launching the Tauri desktop application bundle on macOS, Linux and
//! Windows. Falls back with helpful build instructions when no built artifact
//! is found.
//!
//! Where the app is and how it is opened are functions of the system, taken
//! as an argument: each system's rule is written once, for an install and a
//! checkout alike, and all three are checked on whichever one runs the tests.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Result;

/// Where a Linux build leaves its `AppImage`, under the checkout.
const APPIMAGE_DIR: &str = "target/release/bundle/appimage";

/// Where a Linux build leaves the bare binary, under the checkout.
const LINUX_BINARY: &str = "target/release/gglib-app";

/// A system the desktop app is built for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Os {
    Mac,
    Linux,
    Windows,
}

impl Os {
    /// The one this binary was built for, or `None` on a system the app is
    /// not built for.
    const fn current() -> Option<Self> {
        if cfg!(target_os = "macos") {
            Some(Self::Mac)
        } else if cfg!(target_os = "linux") {
            Some(Self::Linux)
        } else if cfg!(target_os = "windows") {
            Some(Self::Windows)
        } else {
            None
        }
    }
}

/// Execute the `gui` command.
///
/// In development mode, prints instructions for running `cargo tauri dev`.
/// Otherwise, locates and launches the built application bundle for the
/// current platform.
pub(crate) fn execute(dev: bool) -> Result<()> {
    if dev {
        println!("Development mode requires running 'cargo tauri dev' directly");
        return Ok(());
    }

    if gglib_core::paths::is_prebuilt_binary() {
        return launch_prebuilt();
    }

    launch_from_repo(Path::new(env!("GGLIB_REPO_ROOT")))
}

/// The app beside the running binary of a prebuilt install, when it is there:
/// the `.app` bundle (macOS), an `AppImage` or `gglib-app` (Linux), or
/// `gglib-app.exe` (Windows).
fn sibling_artifact(os: Os, exe_dir: &Path) -> Option<PathBuf> {
    match os {
        Os::Mac => Some(exe_dir.join("GGLib GUI.app")).filter(|bundle| bundle.exists()),
        Os::Linux => find_sibling_gui_artifact(exe_dir),
        Os::Windows => Some(exe_dir.join("gglib-app.exe")).filter(|exe| exe.exists()),
    }
}

/// Look for a Linux GUI artifact next to the running binary.
fn find_sibling_gui_artifact(exe_dir: &Path) -> Option<PathBuf> {
    let candidates = std::fs::read_dir(exe_dir).ok()?;
    for entry in candidates.flatten() {
        let path = entry.path();
        if path.is_file()
            && let Some(name) = path.file_name().and_then(|s| s.to_str())
            && (name.ends_with(".AppImage") || name == "gglib-app")
        {
            return Some(path);
        }
    }
    None
}

/// Where a checkout's build output has the app, built or not.
///
/// On Windows that is the bare binary rather than the NSIS output under
/// `bundle/nsis`: that is an installer to run once, not something to launch
/// in place. It mirrors the Linux fallback.
fn repo_artifact(os: Os, repo_root: &Path) -> PathBuf {
    match os {
        Os::Mac => repo_root.join("target/release/bundle/macos/GGLib GUI.app"),
        Os::Linux => find_repo_gui_artifact(repo_root),
        Os::Windows => repo_root.join("target/release/gglib-app.exe"),
    }
}

/// Locate the Linux GUI artifact in the repo build output, preferring any
/// `.AppImage` found in the standard bundle directory and falling back to the
/// raw binary path.
fn find_repo_gui_artifact(repo_root: &Path) -> PathBuf {
    if let Ok(read_dir) = std::fs::read_dir(repo_root.join(APPIMAGE_DIR)) {
        let mut candidates: Vec<PathBuf> = read_dir
            .filter_map(std::result::Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.is_file()
                    && path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .is_some_and(|name| name.ends_with(".AppImage"))
            })
            .collect();

        candidates.sort();
        if let Some(path) = candidates.into_iter().next() {
            return path;
        }
    }

    repo_root.join(LINUX_BINARY)
}

/// The command that opens the app at `artifact`: macOS hands the bundle to
/// `open`, and Linux and Windows run the artifact itself.
fn launch_command(os: Os, artifact: &Path) -> Command {
    match os {
        Os::Mac => {
            let mut open = Command::new("open");
            open.arg(artifact);
            open
        }
        Os::Linux | Os::Windows => Command::new(artifact),
    }
}

/// Open the app at `artifact`. `open` is waited for, since its status says
/// whether the bundle launched; an artifact run directly is left running.
fn launch(os: Os, artifact: &Path) -> Result<()> {
    println!("Launching GGLib GUI...");
    let mut command = launch_command(os, artifact);
    match os {
        Os::Mac => match command.status() {
            Ok(s) if s.success() => Ok(()),
            Ok(s) => anyhow::bail!("Failed to launch GUI (exit code: {:?})", s.code()),
            Err(e) => Err(e.into()),
        },
        Os::Linux | Os::Windows => match command.spawn() {
            Ok(_child) => Ok(()),
            Err(e) if os == Os::Linux && e.kind() == std::io::ErrorKind::PermissionDenied => {
                anyhow::bail!(
                    "Failed to launch GUI: {} (is it executable? try: chmod +x \"{}\")",
                    e,
                    artifact.display()
                )
            }
            Err(e) => Err(e.into()),
        },
    }
}

/// Launch the GUI from a prebuilt standalone binary: whatever
/// [`sibling_artifact`] finds next to the running executable.
fn launch_prebuilt() -> Result<()> {
    let exe_dir = std::env::current_exe()
        .and_then(|p| p.canonicalize())
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf));
    let Some(exe_dir) = exe_dir else {
        anyhow::bail!("Could not determine the directory of the running executable");
    };

    if let Some(os) = Os::current()
        && let Some(artifact) = sibling_artifact(os, &exe_dir)
    {
        return launch(os, &artifact);
    }

    println!("Desktop GUI is not included in this release.");
    println!();
    println!("Use 'gglib web' to open the browser-based interface instead.");
    Ok(())
}

/// Launch the platform-appropriate GUI bundle from a source repo, or say
/// where it was looked for and how to build it.
fn launch_from_repo(repo_root: &Path) -> Result<()> {
    let Some(os) = Os::current() else {
        anyhow::bail!("gglib gui is not supported on this OS yet")
    };

    let artifact = repo_artifact(os, repo_root);
    if artifact.exists() {
        return launch(os, &artifact);
    }

    println!("{}", not_found_line(os, repo_root, &artifact));
    println!();
    println!("To build the GUI, run: make build-tauri");
    println!("Or: npm run tauri:build");
    Ok(())
}

/// What a checkout with no built app is told: where `artifact` was expected,
/// and on Linux the directory an `AppImage` would also have been taken from.
fn not_found_line(os: Os, repo_root: &Path, artifact: &Path) -> String {
    match os {
        Os::Mac | Os::Windows => format!("Desktop GUI not found at: {}", artifact.display()),
        Os::Linux => format!(
            "Desktop GUI not found at: {} (or any *.AppImage in {})",
            repo_root.join(LINUX_BINARY).display(),
            repo_root.join(APPIMAGE_DIR).display()
        ),
    }
}

#[cfg(test)]
#[path = "gui_tests.rs"]
mod tests;
