//! Update command for llama.cpp.

use super::build::build_llama_cpp;
use super::config::BuildConfig;
use super::detect::Acceleration;
use super::install::install_binary;
use anyhow::{Context, Result, bail};
use gglib_core::paths::{llama_cli_path, llama_config_path, llama_cpp_dir, llama_server_path};
use std::io::{self, Write};
use std::process::Command;

// Helper to convert PathError to anyhow::Error
fn path_err<T>(r: Result<T, gglib_core::paths::PathError>) -> Result<T> {
    r.map_err(|e| anyhow::anyhow!("{}", e))
}

/// Check for llama.cpp updates
pub async fn handle_check_updates() -> Result<()> {
    let llama_dir = path_err(llama_cpp_dir())?;
    let binary_path = path_err(llama_server_path())?;

    if !binary_path.exists() {
        println!("llama.cpp is not installed.");
        println!("Run 'gglib llama install' to install it.");
        return Ok(());
    }

    if !llama_dir.exists() {
        println!("Warning: llama.cpp repository not found.");
        println!("Run 'gglib llama rebuild' to reinstall.");
        return Ok(());
    }

    // Load current config
    let config_path = path_err(llama_config_path())?;
    let config = if config_path.exists() {
        BuildConfig::load(&config_path)?
    } else {
        println!("Warning: Build configuration not found.");
        return Ok(());
    };

    println!(
        "Current version: {} ({})",
        config.version,
        config.build_date.format("%Y-%m-%d")
    );
    println!("Acceleration: {}", config.acceleration);
    println!();
    println!("Checking for updates...");

    // Fetch latest from remote
    let status = Command::new("git")
        .args(["-C", llama_dir.to_str().unwrap(), "fetch", "origin"])
        .status()
        .context("Failed to fetch updates")?;

    if !status.success() {
        bail!("Failed to fetch updates from remote");
    }

    // Check if we're behind
    let output = Command::new("git")
        .args([
            "-C",
            llama_dir.to_str().unwrap(),
            "rev-list",
            "--count",
            "HEAD..origin/master",
        ])
        .output()
        .context("Failed to check for updates")?;

    let commits_behind = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u32>()
        .unwrap_or(0);

    if commits_behind == 0 {
        println!("✓ llama.cpp is up to date");
        return Ok(());
    }

    // Get latest commit info
    let output = Command::new("git")
        .args([
            "-C",
            llama_dir.to_str().unwrap(),
            "log",
            "--oneline",
            "-n",
            "5",
            "HEAD..origin/master",
        ])
        .output()
        .context("Failed to get commit log")?;

    let commits = String::from_utf8_lossy(&output.stdout);

    println!("✓ New version available ({} commits ahead)", commits_behind);
    println!();
    println!("Recent changes:");
    for line in commits.lines().take(5) {
        println!("  {}", line);
    }
    println!();
    println!("Run 'gglib llama update' to upgrade");

    Ok(())
}

/// Update llama.cpp to the latest version
pub async fn handle_update() -> Result<()> {
    let llama_dir = path_err(llama_cpp_dir())?;
    let binary_path = path_err(llama_server_path())?;
    let cli_path = path_err(llama_cli_path())?;

    if !binary_path.exists() {
        println!("llama.cpp is not installed.");
        println!("Run 'gglib llama install' to install it.");
        return Ok(());
    }

    if !llama_dir.exists() {
        println!("Error: llama.cpp repository not found.");
        println!("Run 'gglib llama install' to reinstall.");
        return Ok(());
    }

    // Load current config to preserve acceleration type
    let config_path = path_err(llama_config_path())?;
    let old_config = if config_path.exists() {
        Some(BuildConfig::load(&config_path)?)
    } else {
        None
    };

    let acceleration = if let Some(ref config) = old_config {
        match config.acceleration.as_str() {
            "Metal" => Acceleration::Metal,
            "CUDA" => Acceleration::Cuda,
            _ => Acceleration::Cpu,
        }
    } else {
        use super::detect::detect_optimal_acceleration;
        detect_optimal_acceleration()
    };

    println!("Updating llama.cpp...");
    println!();

    if let Some(ref config) = old_config {
        println!("Current version: {}", config.version);
        println!("Build config: {}", config.acceleration);
    }

    println!();
    println!("This will:");
    println!("  - Pull latest llama.cpp changes");
    println!("  - Rebuild with {} support", acceleration.display_name());
    println!("  - Replace current binary");
    println!();
    println!("Current models will NOT be affected.");
    println!();

    print!("Continue? [y/N]: ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    if !input.trim().eq_ignore_ascii_case("y") {
        println!("Update cancelled.");
        return Ok(());
    }

    // Pull latest changes
    println!();
    println!("Pulling latest changes...");
    let status = Command::new("git")
        .args([
            "-C",
            llama_dir.to_str().unwrap(),
            "pull",
            "origin",
            "master",
        ])
        .status()
        .context("Failed to pull updates")?;

    if !status.success() {
        bail!("Failed to pull updates");
    }

    println!("✓ Repository updated");

    // Get new version info
    let output = Command::new("git")
        .args([
            "-C",
            llama_dir.to_str().unwrap(),
            "rev-parse",
            "--short",
            "HEAD",
        ])
        .output()
        .context("Failed to get commit hash")?;
    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();

    let output = Command::new("git")
        .args(["-C", llama_dir.to_str().unwrap(), "rev-parse", "HEAD"])
        .output()
        .context("Failed to get commit SHA")?;
    let commit_sha = String::from_utf8_lossy(&output.stdout).trim().to_string();

    // Rebuild
    build_llama_cpp(&llama_dir, acceleration)?;

    // Install binaries
    install_binary(&llama_dir, "llama-server", &binary_path)?;
    install_binary(&llama_dir, "llama-cli", &cli_path)?;

    // Save new configuration
    let config = BuildConfig::new(version.clone(), commit_sha, acceleration);
    config.save(&config_path)?;

    println!();
    println!("✓ llama.cpp updated successfully!");
    println!("  New version: {}", version);
    println!("  Acceleration: {}", acceleration.display_name());

    Ok(())
}
