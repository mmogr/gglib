//! llama.cpp management command handler.
//!
//! Routes each `LlamaCommand` variant to what carries it out. Installing and
//! updating have adapters of their own beside this file, because they ask a
//! question and draw progress; `status` and `check-updates` are printed by
//! `gglib_runtime::llama`; and uninstalling is short enough to live here.

use anyhow::Result;

use crate::llama_commands::LlamaCommand;
use crate::utils::input;

use super::llama_detect;
use super::llama_install;
use super::llama_update;

/// Dispatch a `llama` sub-command to its handler.
pub(crate) async fn dispatch(command: LlamaCommand) -> Result<()> {
    use gglib_runtime::llama::{handle_check_updates, handle_status};

    match command {
        LlamaCommand::Install {
            cuda,
            metal,
            vulkan,
            force,
            build,
        } => {
            llama_install::handle_install(cuda, metal, vulkan, force, build).await?;
        }
        LlamaCommand::CheckUpdates => {
            handle_check_updates().await?;
        }
        LlamaCommand::Update => {
            llama_update::handle_update().await?;
        }
        LlamaCommand::Status => {
            handle_status().await?;
        }
        LlamaCommand::Rebuild {
            cuda,
            metal,
            vulkan,
        } => {
            llama_install::handle_install(cuda, metal, vulkan, true, true).await?;
        }
        LlamaCommand::Uninstall { force } => {
            uninstall(force).await?;
        }
        LlamaCommand::Detect { json } => {
            llama_detect::execute(json)?;
        }
    }
    Ok(())
}

/// Remove the llama.cpp installation, asking first unless `force`, and print
/// what was removed.
async fn uninstall(force: bool) -> Result<()> {
    use gglib_runtime::llama::{llama_files_present, uninstall_llama};

    if !llama_files_present()? {
        println!("llama.cpp is not installed.");
        return Ok(());
    }

    if !force
        && !input::prompt_confirmation("This will remove llama.cpp and llama-server. Continue?")?
    {
        println!("Uninstall cancelled.");
        return Ok(());
    }

    println!("Removing llama.cpp installation...");

    let outcome = uninstall_llama().await?;
    for path in &outcome.removed_paths {
        println!("✓ Removed {path}");
    }

    println!("llama.cpp uninstalled successfully.");
    Ok(())
}
