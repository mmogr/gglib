//! Finding the `gglib` CLI and launching a detached `gglib daemon run`.
//!
//! Split out of [`super`] so the binary-name resolution has somewhere to be
//! tested, and so the parent module stays inside the file-size ratchet.
//!
//! The app prefers an external daemon over hosting one in-process: it survives
//! a crash of this app, and it is the same process the CLI would have started.
//! That preference is only as good as the lookup below — when it fails, the
//! app silently falls back to hosting, which dies with the window.

use std::path::PathBuf;

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

/// Spawn `gglib daemon run` detached: sibling binary first, then `$PATH`.
pub(super) fn spawn_external_daemon() -> Result<(), String> {
    let name = cli_binary_name();

    let candidate = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(&name)))
        .filter(|p| p.exists());

    let program = match candidate {
        Some(path) => path,
        None => which_gglib().ok_or("no `gglib` binary next to the app or on PATH")?,
    };

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

/// Locate `gglib` on `$PATH` without shelling out.
fn which_gglib() -> Option<PathBuf> {
    let name = cli_binary_name();
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths)
        .map(|dir| dir.join(&name))
        .find(|candidate| candidate.is_file())
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

    /// The name must carry the platform's executable suffix. Joining the bare
    /// string `"gglib"` — which both lookups above used to do — cannot match a
    /// real file on Windows, so `spawn_external_daemon` always failed there and
    /// the app fell back to an in-process daemon that dies with the window.
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

    /// Both lookups must derive their filename from the same place. Them
    /// drifting apart is how the sibling probe gets fixed while the `$PATH`
    /// fallback silently keeps failing.
    ///
    /// Asserted by construction rather than by planting a file on `$PATH`:
    /// `set_var` is process-global and `unsafe` under edition 2024, so a test
    /// that rewrote `$PATH` would be both racy under the parallel harness and
    /// a new `unsafe` block in a crate that has none.
    #[test]
    fn both_lookups_share_one_name_source() {
        let name = cli_binary_name();
        assert_eq!(
            name,
            cli_binary_name(),
            "cli_binary_name must be the single source both lookups call"
        );
        assert!(
            !name.contains(std::path::MAIN_SEPARATOR),
            "{name:?} is a file name, not a path — callers join it onto a dir"
        );
    }
}
