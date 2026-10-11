//! Finding the `gglib` CLI and launching a detached `gglib daemon run`.
//!
//! Separate from [`super`] so the binary-name resolution has somewhere to be
//! tested.
//!
//! The app prefers an external daemon over hosting one in-process: it survives
//! a crash of this app, and it is the same process the CLI would have started.
//! When there is no CLI to launch, the app hosts the daemon itself, which
//! dies with the window.
//!
//! The CLI it launches is the one beside it, never one found on `$PATH`. Every
//! release puts one there: the macOS bundle, the Windows installer, the
//! `AppImage` and the rpm carry their own copy (Tauri's `externalBin`, set by
//! `tauri.sidecar.conf.json` when release.yml bundles), the deb depends on the
//! `gglib` package of the same version, which installs `/usr/bin/gglib` beside
//! `/usr/bin/gglib-app`, and the archives hold both. A build from a checkout
//! has `target/<profile>/gglib` beside it once the CLI is built. So the daemon
//! the app starts is always the app's own version, whatever else is on
//! `$PATH`.

use std::path::{Path, PathBuf};

/// The `gglib` CLI's filename on this platform.
///
/// `EXE_SUFFIX` is `".exe"` on Windows and empty everywhere else.
///
/// The house idiom for this is a `#[cfg(target_os = "windows")]` pair of
/// literals — see `gglib_core::paths::llama_server_path`. The constant is used
/// here instead, deliberately: it keeps the lookup a single code path on every
/// platform, and it lets the test below assert something true everywhere
/// rather than splitting into per-target arms that only ever run on one.
fn cli_binary_name() -> String {
    format!("gglib{}", std::env::consts::EXE_SUFFIX)
}

/// The `gglib` CLI in the directory `exe` is in, when there is one.
fn sibling_cli(exe: &Path) -> Option<PathBuf> {
    exe.parent()
        .map(|dir| dir.join(cli_binary_name()))
        .filter(|p| p.is_file())
}

/// Spawn `gglib daemon run` detached, from the CLI beside this app.
pub(super) fn spawn_external_daemon() -> Result<(), String> {
    let program = std::env::current_exe()
        .ok()
        .as_deref()
        .and_then(sibling_cli)
        .ok_or("no `gglib` binary beside the app")?;

    let log = daemon_log_file().map_err(|e| format!("opening daemon log: {e}"))?;

    let mut cmd = std::process::Command::new(program);
    cmd.args(["daemon", "run"])
        .stdin(std::process::Stdio::null())
        .stdout(log.try_clone().map_err(|e| e.to_string())?)
        .stderr(log);

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }

    cmd.spawn().map_err(|e| e.to_string())?;
    Ok(())
}

/// The log file an auto-launched daemon writes to.
fn daemon_log_file() -> std::io::Result<std::fs::File> {
    let dir = gglib_core::paths::data_root()
        .map_err(|e| std::io::Error::other(e.to_string()))?
        .join("logs");
    std::fs::create_dir_all(&dir)?;
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("daemon.log"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The name must carry the platform's executable suffix. The bare string
    /// `"gglib"` cannot match a real file on Windows, so `spawn_external_daemon`
    /// would always fail there and the app fall back to an in-process daemon
    /// that dies with the window.
    #[test]
    fn cli_binary_name_carries_the_platform_exe_suffix() {
        let name = cli_binary_name();

        assert!(
            name.starts_with("gglib"),
            "{name:?} should be the gglib CLI"
        );
        assert!(
            name.ends_with(std::env::consts::EXE_SUFFIX),
            "{name:?} should end with this platform's EXE_SUFFIX"
        );

        #[cfg(target_os = "windows")]
        assert_eq!(name, "gglib.exe");

        #[cfg(not(target_os = "windows"))]
        assert_eq!(name, "gglib");
    }

    /// The CLI beside the app is found, and nothing is when it is absent:
    /// a directory holding only the app means hosting in-process, not a
    /// search elsewhere.
    #[test]
    fn the_cli_is_taken_from_beside_the_app_or_not_at_all() {
        let dir = tempfile::tempdir().expect("tempdir");
        let app = dir.path().join("gglib-app");
        std::fs::write(&app, b"").expect("the app");

        assert_eq!(sibling_cli(&app), None);

        let cli = dir.path().join(cli_binary_name());
        std::fs::write(&cli, b"").expect("the cli");
        assert_eq!(sibling_cli(&app), Some(cli));
    }

    /// A directory by the CLI's name is not a CLI.
    #[test]
    fn a_directory_named_like_the_cli_is_not_taken() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir(dir.path().join(cli_binary_name())).expect("a directory");
        assert_eq!(sibling_cli(&dir.path().join("gglib-app")), None);
    }
}
