//! Binary validation and status checking for llama-server.

use anyhow::Result;
use gglib_core::domain::RuntimeKind;
use std::path::Path;

/// Validate that the llama-server binary is functional: the check both
/// engines share, [`crate::engine::validate_engine_at`].
pub fn validate_llama_binary(path: &Path) -> Result<()> {
    crate::engine::validate_engine_at(RuntimeKind::Llama, path)?;
    Ok(())
}

/// Handle the status command.
///
/// A printer over [`super::llama_status`] — the same value the GUI's System
/// tab renders, so the two surfaces cannot report different installs.
pub async fn handle_status() -> Result<()> {
    for line in status_lines(&super::llama_status()?) {
        println!("{line}");
    }
    Ok(())
}

/// What `gglib config llama status` prints for `status`: its state in the
/// word every surface uses, then the binary, the pin, its health, the
/// install's record and what the binary reports. Not installed says the
/// command that installs it; a binary that fails its check stops at why.
fn status_lines(status: &super::LlamaStatus) -> Vec<String> {
    let state =
        gglib_core::domain::EngineState::of(status.installed, status.health_error.is_some());
    let mut lines = vec![format!("Status: {state}")];
    if !status.installed {
        lines.push(String::new());
        lines.push("Run 'gglib config llama install' to install llama.cpp".to_owned());
        return lines;
    }

    lines.push(format!("Binary: {}", status.binary_path));
    lines.extend(crate::engine::standing_line(&status.pin));

    match &status.health_error {
        None => lines.push("Health: ✓ Functional".to_owned()),
        Some(e) => {
            lines.push(format!("Health: ✗ Error - {e}"));
            return lines;
        }
    }

    lines.push(String::new());
    match (&status.build, &status.prebuilt, &status.build_error) {
        (Some(build), _, _) => {
            lines.push("Build Information:".to_owned());
            lines.push(format!("  Version: {}", build.version));
            lines.push(format!("  Commit: {}", build.commit_sha));
            lines.push(format!("  Built: {}", human_time(&build.build_date)));
            lines.push(format!("  Acceleration: {}", build.acceleration));
            lines.push(format!("  CMake flags: {}", build.cmake_flags.join(" ")));
        }
        (None, Some(prebuilt), _) => {
            lines.push("Pre-built download:".to_owned());
            lines.push(format!("  Version: {}", prebuilt.version));
            lines.push(format!("  Platform: {}", prebuilt.platform));
            lines.push(format!(
                "  Installed: {}",
                human_time(&prebuilt.installed_at)
            ));
        }
        (None, None, Some(e)) => lines.push(format!("Warning: Could not load build config: {e}")),
        (None, None, None) => lines.push("Warning: Build configuration not found".to_owned()),
    }

    if let Some(caps) = &status.runtime {
        lines.push(String::new());
        lines.push(format!("Binary version: {}", caps.version_line));
        lines.push(format!("  Build: {}", caps.build_label()));
        lines.push(format!("  Native capabilities: {}", caps.flags));
    }
    lines
}

/// An RFC 3339 time as this command has always printed one. The status
/// carries RFC 3339 for the wire, so it is turned back at the print site;
/// anything that is not RFC 3339 is printed as it stands.
fn human_time(rfc3339: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(rfc3339).map_or_else(
        |_| rfc3339.to_string(),
        |time| {
            time.with_timezone(&chrono::Utc)
                .format("%Y-%m-%d %H:%M:%S UTC")
                .to_string()
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use gglib_core::paths::LLAMA_INSTALL_COMMAND;
    use std::time::Duration;
    // `tempdir` is used by the always-compiled tests; `fs` only by the
    // `#[cfg(unix)]` one that sets a file's permission bits.
    #[cfg(unix)]
    use std::fs;
    use tempfile::tempdir;

    /// An engine that never answers `--version` is reported within the
    /// wait, not waited on (#1984).
    #[test]
    fn an_engine_that_never_answers_is_reported_within_the_wait() {
        let dir = tempdir().unwrap();
        let engine = gglib_test_proc::install(dir.path(), "llama-server", "sleep 60");
        let wait = Duration::from_millis(500);

        let (sent, done) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = sent.send(
                crate::engine::validate_binary(&engine, LLAMA_INSTALL_COMMAND, wait)
                    .map(drop)
                    .map_err(|e| e.to_string()),
            );
        });
        let answer = done
            .recv_timeout(Duration::from_secs(15))
            .expect("validate still waiting 15 s on an engine that never exits");

        let refusal = answer.expect_err("an engine that never answers is not working");
        assert!(refusal.contains("did not answer"), "{refusal}");
    }

    /// An engine the loader cannot start for want of a library is said to be
    /// missing that library, not to be corrupted (#1959 A1). The stand-in
    /// prints what glibc's loader prints and exits 127, as it does.
    #[test]
    fn an_engine_missing_a_library_says_which() {
        let dir = tempdir().unwrap();
        let engine = gglib_test_proc::install(
            dir.path(),
            "llama-server",
            "eprint ./llama-server: error while loading shared libraries: libvulkan.so.1: \
             cannot open shared object file: No such file or directory\nexit 127",
        );

        let refusal = validate_llama_binary(&engine)
            .expect_err("an engine that cannot start is not working")
            .to_string();

        assert!(
            refusal.contains("the library libvulkan.so.1 it needs is missing"),
            "{refusal}"
        );
        assert!(!refusal.contains("corrupted"), "{refusal}");
    }

    #[test]
    fn test_validate_nonexistent() {
        let path = Path::new("/nonexistent/llama-server");
        let result = validate_llama_binary(path);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_not_a_file() {
        let dir = tempdir().unwrap();
        let result = validate_llama_binary(dir.path());
        assert!(result.is_err());
    }

    #[test]
    fn a_recorded_time_prints_as_a_utc_timestamp_and_anything_else_as_it_stands() {
        assert_eq!(
            human_time("2026-08-10T14:34:56+02:00"),
            "2026-08-10 12:34:56 UTC"
        );
        assert_eq!(human_time("last Tuesday"), "last Tuesday");
    }

    // Unix only: the system refuses to run a file whose permission bits do
    // not let it.
    #[cfg(unix)]
    #[test]
    fn test_validate_not_executable() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test-binary");
        fs::write(&file_path, "#!/bin/sh\necho test").unwrap();

        // Set non-executable permissions
        let mut perms = fs::metadata(&file_path).unwrap().permissions();
        perms.set_mode(0o644);
        fs::set_permissions(&file_path, perms).unwrap();

        // The system's refusal to run it is what says so: no permission bits
        // are read by gglib.
        let refusal = validate_llama_binary(&file_path)
            .expect_err("a file that is not executable is not working")
            .to_string();
        assert!(
            refusal.contains("is not a program this system can run"),
            "{refusal}"
        );
    }

    /// What `status` reports for an install that is `installed`, and if so
    /// fails its check with `health_error`.
    fn status(installed: bool, health_error: Option<&str>) -> super::super::LlamaStatus {
        use crate::engine::{PinStanding, PinState};
        super::super::LlamaStatus {
            installed,
            binary_path: "/data/.llama/bin/llama-server".to_owned(),
            config_path: "/data/.llama/llama-config.json".to_owned(),
            healthy: installed && health_error.is_none(),
            health_error: health_error.map(str::to_owned),
            fix: None,
            build: None,
            build_error: None,
            prebuilt: installed.then(|| super::super::LlamaPrebuiltInfo {
                version: "b11669".to_owned(),
                platform: "macOS ARM64 (Metal)".to_owned(),
                installed_at: "2026-10-31T06:24:56+00:00".to_owned(),
            }),
            runtime: None,
            pin: PinState {
                standing: if installed {
                    PinStanding::AtPin
                } else {
                    PinStanding::Missing
                },
                installed_release: installed.then(|| "b11669".to_owned()),
                target_release: Some("b11669".to_owned()),
                built: installed.then_some(false),
                rebuilds: false,
                update_downloads: !installed,
            },
        }
    }

    /// The status line is the engine's state in the word every surface
    /// uses: installed, installed and not working, or not installed.
    #[test]
    fn the_status_says_the_engines_state_in_its_word() {
        assert_eq!(
            status_lines(&status(true, None)),
            [
                "Status: Installed",
                "Binary: /data/.llama/bin/llama-server",
                "Pinned release: b11669, which is the one installed.",
                "Health: \u{2713} Functional",
                "",
                "Pre-built download:",
                "  Version: b11669",
                "  Platform: macOS ARM64 (Metal)",
                "  Installed: 2026-10-31 06:24:56 UTC",
            ]
        );
        assert_eq!(
            status_lines(&status(true, Some("llama-server will not start"))),
            [
                "Status: Installed, not working",
                "Binary: /data/.llama/bin/llama-server",
                "Pinned release: b11669, which is the one installed.",
                "Health: \u{2717} Error - llama-server will not start",
            ]
        );
        assert_eq!(
            status_lines(&status(false, None)),
            [
                "Status: Not installed",
                "",
                "Run 'gglib config llama install' to install llama.cpp",
            ]
        );
    }
}
