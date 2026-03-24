//! GUI launch handler.
//!
//! Handles launching the Tauri desktop application bundle on macOS and Linux.
//! Falls back with helpful build instructions when no built artifact is found.

use anyhow::Result;

/// Execute the `gui` command.
///
/// In development mode, prints instructions for running `cargo tauri dev`.
/// Otherwise, locates and launches the built application bundle for the
/// current platform.
pub fn execute(dev: bool) -> Result<()> {
    if dev {
        println!("Development mode requires running 'cargo tauri dev' directly");
        return Ok(());
    }
    let repo_root = std::path::PathBuf::from(env!("GGLIB_REPO_ROOT"));
    launch(&repo_root)
}

/// Locate the Linux GUI artifact, preferring any `.AppImage` found in the
/// standard bundle directory and falling back to the raw binary path.
#[cfg(target_os = "linux")]
fn find_linux_gui_artifact(repo_root: &std::path::Path) -> std::path::PathBuf {
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

/// Launch the platform-appropriate GUI bundle.
fn launch(repo_root: &std::path::Path) -> Result<()> {
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
        let artifact = find_linux_gui_artifact(repo_root);
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
        return Ok(());
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = repo_root;
        anyhow::bail!("gglib gui is not supported on this OS yet")
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::find_linux_gui_artifact;

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

        let chosen = find_linux_gui_artifact(&root);
        assert_eq!(chosen, appimage);
    }

    #[test]
    fn linux_gui_artifact_falls_back_to_binary_when_no_appimage() {
        let root = make_temp_dir("gglib_cli_gui");
        let chosen = find_linux_gui_artifact(&root);
        assert_eq!(chosen, root.join("target/release/gglib-app"));
    }
}
