//! llama.cpp management command handler.
//!
//! Routes each `LlamaCommand` variant to what carries it out. Installing,
//! which is also updating, is `engine`'s, shared with stable-diffusion.cpp;
//! `status` is printed by `gglib_runtime::llama`; and uninstalling is short
//! enough to live here.

use anyhow::Result;

use crate::bootstrap::CliContext;
use crate::llama_commands::LlamaCommand;
use crate::utils::input;
use gglib_core::domain::RuntimeKind;

use super::engine;
use super::llama_detect;

/// Dispatch a `llama` sub-command to its handler.
pub(crate) async fn dispatch(ctx: &CliContext, command: LlamaCommand) -> Result<()> {
    match command {
        LlamaCommand::Install { force, build } => {
            engine::install(ctx, RuntimeKind::Llama, force, build).await?;
        }
        LlamaCommand::Status => {
            gglib_runtime::llama::handle_status().await?;
        }
        LlamaCommand::Uninstall { force } => {
            uninstall(ctx, force).await?;
        }
        LlamaCommand::Detect { json } => {
            llama_detect::execute(json)?;
        }
    }
    Ok(())
}

/// Remove the llama.cpp installation, asking first unless `force`, and print
/// what was removed. Refused while the daemon runs a chat model on it, as
/// stable-diffusion.cpp's removal is while it draws.
async fn uninstall(ctx: &CliContext, force: bool) -> Result<()> {
    use gglib_runtime::llama::{llama_files_present, uninstall_llama};

    if !llama_files_present()? {
        println!("llama.cpp is not installed.");
        return Ok(());
    }
    if let Some(model) = engine::serving_on_the_daemon(ctx, RuntimeKind::Llama).await? {
        anyhow::bail!(engine::removal_refusal(RuntimeKind::Llama, &model));
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
