//! GUI launch handler.
//!
//! Handles launching the Tauri desktop application bundle on macOS, Linux and
//! Windows. Falls back with helpful build instructions when no built artifact
//! is found.

use anyhow::Result;

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

    let repo_root = std::path::PathBuf::from(env!("GGLIB_REPO_ROOT"));
    launch_from_repo(&repo_root)
}

/// Look for a GUI artifact next to the running binary (prebuilt installs).
#[cfg(target_os = "linux")]
fn find_sibling_gui_artifact(exe_dir: &std::path::Path) -> Option<std::path::PathBuf> {
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

/// Locate the Linux GUI artifact in the repo build output, preferring any
/// `.AppImage` found in the standard bundle directory and falling back to the
/// raw binary path.
#[cfg(target_os = "linux")]
fn find_repo_gui_artifact(repo_root: &std::path::Path) -> std::path::PathBuf {
    let appimage_dir = repo_root.join("target/release/bundle/appimage");
    if let Ok(read_dir) = std::fs::read_dir(&appimage_dir) {
        let mut candidates: Vec<std::path::PathBuf> = read_dir
            .filter_map(|entry| entry.ok())
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

    repo_root.join("target/release/gglib-app")
}

/// Locate the Windows GUI artifact in the repo build output.
///
/// The bare binary rather than the NSIS output under `bundle/nsis`: that is an
/// installer to run once, not something to launch in place. Mirrors the Linux
/// fallback above.
#[cfg(target_os = "windows")]
fn find_repo_gui_artifact(repo_root: &std::path::Path) -> std::path::PathBuf {
    repo_root.join("target/release/gglib-app.exe")
}

/// Launch the GUI from a prebuilt standalone binary.
///
/// Looks for the `.app` bundle (macOS), an AppImage or `gglib-app` (Linux), or
/// `gglib-app.exe` (Windows) next to the running executable.
fn launch_prebuilt() -> Result<()> {
    let exe_dir = std::env::current_exe()
        .and_then(|p| p.canonicalize())
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()));

    let exe_dir = match exe_dir {
        Some(d) => d,
        None => {
            anyhow::bail!("Could not determine the directory of the running executable");
        }
    };

    #[cfg(target_os = "macos")]
    {
        let app_bundle = exe_dir.join("GGLib GUI.app");
        if app_bundle.exists() {
            println!("Launching GGLib GUI...");
            let status = std::process::Command::new("open").arg(&app_bundle).status();
            return match status {
                Ok(s) if s.success() => Ok(()),
                Ok(s) => anyhow::bail!("Failed to launch GUI (exit code: {:?})", s.code()),
                Err(e) => Err(e.into()),
            };
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Some(artifact) = find_sibling_gui_artifact(&exe_dir) {
            println!("Launching GGLib GUI...");
            let spawned = std::process::Command::new(&artifact).spawn();
            return match spawned {
                Ok(_child) => Ok(()),
                Err(e) => {
                    if e.kind() == std::io::ErrorKind::PermissionDenied {
                        anyhow::bail!(
                            "Failed to launch GUI: {} (is it executable? try: chmod +x \"{}\")",
                            e,
                            artifact.display()
                        );
                    }
                    Err(e.into())
                }
            };
        }
    }

    #[cfg(target_os = "windows")]
    {
        let artifact = exe_dir.join("gglib-app.exe");
        if artifact.exists() {
            println!("Launching GGLib GUI...");
            return match std::process::Command::new(&artifact).spawn() {
                Ok(_child) => Ok(()),
                Err(e) => Err(e.into()),
            };
        }
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    let _ = exe_dir;

    println!("Desktop GUI is not included in this release.");
    println!();
    println!("Use 'gglib web' to open the browser-based interface instead.");
    Ok(())
}

/// Launch the platform-appropriate GUI bundle from a source repo.
fn launch_from_repo(repo_root: &std::path::Path) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let app_bundle = repo_root.join("target/release/bundle/macos/GGLib GUI.app");
        if app_bundle.exists() {
            println!("Launching GGLib GUI...");
            let status = std::process::Command::new("open").arg(&app_bundle).status();
            return match status {
                Ok(s) if s.success() => Ok(()),
                Ok(s) => anyhow::bail!("Failed to launch GUI (exit code: {:?})", s.code()),
                Err(e) => Err(e.into()),
            };
        }
        println!("Desktop GUI not found at: {}", app_bundle.display());
        println!();
        println!("To build the GUI, run: make build-tauri");
        println!("Or: npm run tauri:build");
        Ok(())
    }

    #[cfg(target_os = "linux")]
    {
        let artifact = find_repo_gui_artifact(repo_root);
        if artifact.exists() {
            println!("Launching GGLib GUI...");
            let spawned = std::process::Command::new(&artifact).spawn();
            return match spawned {
                Ok(_child) => Ok(()),
                Err(e) => {
                    if e.kind() == std::io::ErrorKind::PermissionDenied {
                        anyhow::bail!(
                            "Failed to launch GUI: {} (is it executable? try: chmod +x \"{}\")",
                            e,
                            artifact.display()
                        );
                    }
                    Err(e.into())
                }
            };
        }
        let appimage_dir = repo_root.join("target/release/bundle/appimage");
        println!(
            "Desktop GUI not found at: {} (or any *.AppImage in {})",
            repo_root.join("target/release/gglib-app").display(),
            appimage_dir.display()
        );
        println!();
        println!("To build the GUI, run: make build-tauri");
        println!("Or: npm run tauri:build");
        Ok(())
    }

    #[cfg(target_os = "windows")]
    {
        let artifact = find_repo_gui_artifact(repo_root);
        if artifact.exists() {
            println!("Launching GGLib GUI...");
            return match std::process::Command::new(&artifact).spawn() {
                Ok(_child) => Ok(()),
                Err(e) => Err(e.into()),
            };
        }
        println!("Desktop GUI not found at: {}", artifact.display());
        println!();
        println!("To build the GUI, run: make build-tauri");
        println!("Or: npm run tauri:build");
        Ok(())
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        let _ = repo_root;
        anyhow::bail!("gglib gui is not supported on this OS yet")
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::find_repo_gui_artifact;

    fn make_temp_dir(prefix: &str) -> std::path::PathBuf {
        let mut base = std::env::temp_dir();
        base.push(format!(
            "{}_{}_{}",
            prefix,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&base).unwrap();
        base
    }

    #[test]
    fn linux_gui_artifact_prefers_any_appimage() {
        let root = make_temp_dir("gglib_cli_gui");
        let appimage_dir = root.join("target/release/bundle/appimage");
        std::fs::create_dir_all(&appimage_dir).unwrap();

        let appimage = appimage_dir.join("GGLib GUI_0.2.4_amd64.AppImage");
        std::fs::write(&appimage, b"stub").unwrap();

        let chosen = find_repo_gui_artifact(&root);
        assert_eq!(chosen, appimage);
    }

    #[test]
    fn linux_gui_artifact_falls_back_to_binary_when_no_appimage() {
        let root = make_temp_dir("gglib_cli_gui");
        let chosen = find_repo_gui_artifact(&root);
        assert_eq!(chosen, root.join("target/release/gglib-app"));
    }
}

#[cfg(all(test, target_os = "windows"))]
mod windows_tests {
    use super::find_repo_gui_artifact;

    /// The Windows lookup must name the executable with its extension. Before
    /// this arm existed, `gglib gui` on Windows printed "Desktop GUI is not
    /// included in this release" even with `gglib-app.exe` sitting beside it,
    /// because there was no Windows branch at all.
    ///
    /// This runs nowhere in CI — `cli-cross-os` is `cargo test --no-run` and
    /// `clippy-cross-os` only lints — so it is compiled and linted rather than
    /// executed. That still catches the errors those jobs exist for.
    #[test]
    fn windows_repo_artifact_is_the_exe() {
        let root = std::path::Path::new("C:\\repo");
        let chosen = find_repo_gui_artifact(root);

        assert_eq!(chosen, root.join("target/release/gglib-app.exe"));
        assert!(
            chosen.to_string_lossy().ends_with(".exe"),
            "{} should name a Windows executable",
            chosen.display()
        );
    }
}
