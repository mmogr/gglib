//! Llama.cpp uninstall and rebuild handlers.

use anyhow::Result;
use gglib_core::paths::gglib_data_dir;
use serde::Serialize;
use std::io::{self, Write};

/// What an uninstall removed.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UninstallOutcome {
    /// False when there was nothing to remove; `removedPaths` is then empty.
    pub was_installed: bool,
    pub removed_paths: Vec<String>,
}

/// Remove the llama.cpp installation: the source checkout, the binary
/// directory and the build configuration.
///
/// Note this removes the whole `bin/` directory, so `llama-bench` goes with
/// `llama-server` — everything a source build installs. Unconditional; the
/// confirmation belongs to the caller.
pub async fn uninstall_llama() -> Result<UninstallOutcome> {
    let gglib_dir = gglib_data_dir()?;
    let llama_dir = gglib_dir.join("llama.cpp");
    let bin_dir = gglib_dir.join("bin");
    let config_path = gglib_dir.join("llama-config.json");

    if !llama_dir.exists() && !bin_dir.exists() {
        return Ok(UninstallOutcome {
            was_installed: false,
            removed_paths: Vec::new(),
        });
    }

    let mut removed_paths = Vec::new();

    if llama_dir.exists() {
        std::fs::remove_dir_all(&llama_dir)?;
        removed_paths.push(llama_dir.display().to_string());
    }

    if bin_dir.exists() {
        std::fs::remove_dir_all(&bin_dir)?;
        removed_paths.push(bin_dir.display().to_string());
    }

    if config_path.exists() {
        std::fs::remove_file(&config_path)?;
        removed_paths.push(config_path.display().to_string());
    }

    Ok(UninstallOutcome {
        was_installed: true,
        removed_paths,
    })
}

/// Handle the uninstall command.
///
/// Prompts unless `force`, then prints what [`uninstall_llama`] removed.
pub async fn handle_uninstall(force: bool) -> Result<()> {
    let gglib_dir = gglib_data_dir()?;
    if !gglib_dir.join("llama.cpp").exists() && !gglib_dir.join("bin").exists() {
        println!("llama.cpp is not installed.");
        return Ok(());
    }

    if !force {
        print!("This will remove llama.cpp and llama-server. Continue? [y/N]: ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Uninstall cancelled.");
            return Ok(());
        }
    }

    println!("Removing llama.cpp installation...");

    let outcome = uninstall_llama().await?;
    for path in &outcome.removed_paths {
        println!("✓ Removed {}", path);
    }

    println!("llama.cpp uninstalled successfully.");
    Ok(())
}
