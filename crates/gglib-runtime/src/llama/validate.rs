//! Binary validation and status checking for llama-server.

use anyhow::{Context, Result, bail};
use gglib_core::utils::process::cmd;
use std::path::Path;

/// Validate that the llama-server binary is functional
pub fn validate_llama_binary(path: &Path) -> Result<()> {
    validate_binary(path, "llama-server")
}

fn validate_binary(path: &Path, binary_name: &str) -> Result<()> {
    if !path.exists() {
        bail!(
            "{} not found at: {}\n\nRun 'gglib config llama install' to install it.",
            binary_name,
            path.display()
        );
    }

    if !path.is_file() {
        bail!("{} path is not a file: {}", binary_name, path.display());
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = path.metadata().context("Failed to read binary metadata")?;
        let perms = metadata.permissions();
        if perms.mode() & 0o111 == 0 {
            bail!("{} is not executable: {}", binary_name, path.display());
        }
    }

    let output = cmd(path)
        .arg("--version")
        .output()
        .with_context(|| format!("Failed to execute {}", binary_name))?;

    if !output.status.success() {
        bail!(
            "{} binary appears corrupted: {}\n\nRun 'gglib config llama rebuild' to fix.",
            binary_name,
            path.display()
        );
    }

    Ok(())
}

/// Handle the status command.
///
/// A printer over [`super::llama_status`] — the same value the GUI's System
/// tab renders, so the two surfaces cannot report different installs.
pub async fn handle_status() -> Result<()> {
    let status = super::llama_status()?;

    if !status.installed {
        println!("Status: Not installed");
        println!();
        println!("Run 'gglib config llama install' to install llama.cpp");
        return Ok(());
    }

    println!("Status: Installed");
    println!("Binary: {}", status.binary_path);

    match &status.health_error {
        None => println!("Health: ✓ Functional"),
        Some(e) => {
            println!("Health: ✗ Error - {}", e);
            return Ok(());
        }
    }

    match (&status.build, &status.build_error) {
        (Some(build), _) => {
            println!();
            println!("Build Information:");
            println!("  Version: {}", build.version);
            println!("  Commit: {}", build.commit_sha);
            // The DTO carries RFC 3339 for the wire; this command has always
            // printed a human timestamp, so format it back at the print site.
            match chrono::DateTime::parse_from_rfc3339(&build.build_date) {
                Ok(dt) => println!(
                    "  Built: {}",
                    dt.with_timezone(&chrono::Utc)
                        .format("%Y-%m-%d %H:%M:%S UTC")
                ),
                Err(_) => println!("  Built: {}", build.build_date),
            }
            println!("  Acceleration: {}", build.acceleration);
            println!("  CMake flags: {}", build.cmake_flags.join(" "));
        }
        (None, Some(e)) => {
            println!();
            println!("Warning: Could not load build config: {}", e);
        }
        (None, None) => {
            println!();
            println!("Warning: Build configuration not found");
        }
    }

    if let Some(caps) = &status.runtime {
        println!();
        println!("Binary version: {}", caps.version_line);
        match caps.build {
            Some(build) => println!("  Build: b{build}"),
            None => println!("  Build: unidentified — gglib will apply every compensation"),
        }
        println!("  Native capabilities: {}", caps.flags);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

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

        let result = validate_llama_binary(&file_path);
        assert!(result.is_err());
    }
}
