#![doc = include_str!(concat!(env!("OUT_DIR"), "/commands_llama_docs.md"))]

mod build;
mod config;
mod deps;
mod detect;
mod download;
mod ensure;
mod install;
mod update;
mod validate;

pub use download::{check_prebuilt_availability, download_prebuilt_binaries, PrebuiltAvailability};

pub use ensure::ensure_llama_initialized;
pub use install::handle_install;
pub use update::{handle_check_updates, handle_update};
pub use validate::{handle_status, validate_llama_binary, validate_llama_cli_binary};

use anyhow::Result;

/// Handle the rebuild command
pub async fn handle_rebuild(cuda: bool, metal: bool, cpu_only: bool) -> Result<()> {
    // Rebuild always builds from source (that's the point of rebuild)
    install::handle_install(cuda, metal, cpu_only, true, true).await
}

/// Handle the uninstall command
pub async fn handle_uninstall(force: bool) -> Result<()> {
    use crate::utils::paths::get_gglib_data_dir;
    use std::io::{self, Write};

    let gglib_dir = get_gglib_data_dir()?;
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
