//! Binary validation and status checking for llama-server.

use super::config::BuildConfig;
use anyhow::{Context, Result, bail};
use gglib_core::paths::{llama_config_path, llama_server_path};
use std::path::Path;
use std::process::Command;

/// Validate that the llama-server binary is functional
pub fn validate_llama_binary(path: &Path) -> Result<()> {
    validate_binary(path, "llama-server")
}

/// Validate that the llama-cli binary is functional
pub fn validate_llama_cli_binary(path: &Path) -> Result<()> {
    validate_binary(path, "llama-cli")
}

fn validate_binary(path: &Path, binary_name: &str) -> Result<()> {
    if !path.exists() {
        bail!(
            "{} not found at: {}\n\nRun 'gglib llama install' to install it.",
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

    let output = Command::new(path)
        .arg("--version")
        .output()
        .with_context(|| format!("Failed to execute {}", binary_name))?;

    if !output.status.success() {
        bail!(
            "{} binary appears corrupted: {}\n\nRun 'gglib llama rebuild' to fix.",
            binary_name,
            path.display()
        );
    }

    Ok(())
}

/// Handle the status command
pub async fn handle_status() -> Result<()> {
    let binary_path = llama_server_path().map_err(|e| anyhow::anyhow!("{}", e))?;
    let config_path = llama_config_path().map_err(|e| anyhow::anyhow!("{}", e))?;

    if !binary_path.exists() {
        println!("Status: Not installed");
        println!();
        println!("Run 'gglib llama install' to install llama.cpp");
        return Ok(());
    }

    println!("Status: Installed");
    println!("Binary: {}", binary_path.display());

    // Validate binary
    match validate_llama_binary(&binary_path) {
        Ok(_) => println!("Health: ✓ Functional"),
        Err(e) => {
            println!("Health: ✗ Error - {}", e);
            return Ok(());
        }
    }

    // Load and display config
    if config_path.exists() {
        match BuildConfig::load(&config_path) {
            Ok(config) => {
                println!();
                println!("Build Information:");
                println!("  Version: {}", config.version);
                println!("  Commit: {}", config.commit_sha);
                println!(
                    "  Built: {}",
                    config.build_date.format("%Y-%m-%d %H:%M:%S UTC")
                );
                println!("  Acceleration: {}", config.acceleration);
                println!("  CMake flags: {}", config.cmake_flags.join(" "));
            }
            Err(e) => {
                println!();
                println!("Warning: Could not load build config: {}", e);
            }
        }
    } else {
        println!();
        println!("Warning: Build configuration not found");
    }

    // Get binary version
    if let Ok(output) = Command::new(&binary_path).arg("--version").output()
        && output.status.success()
    {
        let version = String::from_utf8_lossy(&output.stdout);
        if let Some(first_line) = version.lines().next() {
            println!();
            println!("Binary version: {}", first_line.trim());
        }
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
