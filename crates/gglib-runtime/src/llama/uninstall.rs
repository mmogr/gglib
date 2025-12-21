//! Llama.cpp uninstall and rebuild handlers.

use anyhow::Result;
use gglib_core::paths::gglib_data_dir;
use std::io::{self, Write};

use super::handle_install;

/// Handle the rebuild command.
///
/// Rebuilds llama.cpp from source with the specified acceleration options.
/// This always forces a fresh build, ignoring any cached binaries.
pub async fn handle_rebuild(cuda: bool, metal: bool, cpu_only: bool) -> Result<()> {
    // Rebuild always builds from source (that's the point of rebuild)
    // force=true, build=true
    handle_install(cuda, metal, cpu_only, true, true).await
}

/// Handle the uninstall command.
///
/// Removes the llama.cpp installation including binaries and configuration.
/// If `force` is false, prompts the user for confirmation.
pub async fn handle_uninstall(force: bool) -> Result<()> {
    let gglib_dir = gglib_data_dir()?;
    let llama_dir = gglib_dir.join("llama.cpp");
    let bin_dir = gglib_dir.join("bin");

    if !llama_dir.exists() && !bin_dir.exists() {
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

    if llama_dir.exists() {
        std::fs::remove_dir_all(&llama_dir)?;
        println!("✓ Removed {}", llama_dir.display());
    }

    if bin_dir.exists() {
        std::fs::remove_dir_all(&bin_dir)?;
        println!("✓ Removed {}", bin_dir.display());
    }

    let config_path = gglib_dir.join("llama-config.json");
    if config_path.exists() {
        std::fs::remove_file(&config_path)?;
        println!("✓ Removed configuration");
    }

    println!("llama.cpp uninstalled successfully.");
    Ok(())
}
