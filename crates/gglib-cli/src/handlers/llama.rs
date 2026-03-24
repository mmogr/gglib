//! llama.cpp management command handler.
//!
//! Thin dispatcher that routes `LlamaCommand` variants to the appropriate
//! functions in `gglib_runtime::llama`. Contains no business logic — all
//! installation and update behaviour lives in the runtime crate.

use anyhow::Result;

use crate::llama_commands::LlamaCommand;

/// Dispatch a `llama` sub-command to the appropriate `gglib_runtime` handler.
pub async fn dispatch(command: LlamaCommand) -> Result<()> {
    use gglib_runtime::llama::{
        handle_check_updates, handle_install, handle_rebuild, handle_status, handle_uninstall,
        handle_update,
    };

    match command {
        LlamaCommand::Install {
            cuda,
            metal,
            vulkan,
            force,
            build,
        } => {
            handle_install(cuda, metal, vulkan, force, build).await?;
        }
        LlamaCommand::CheckUpdates => {
            handle_check_updates().await?;
        }
        LlamaCommand::Update => {
            handle_update().await?;
        }
        LlamaCommand::Status => {
            handle_status().await?;
        }
        LlamaCommand::Rebuild {
            cuda,
            metal,
            vulkan,
        } => {
            handle_rebuild(cuda, metal, vulkan).await?;
        }
        LlamaCommand::Uninstall { force } => {
            handle_uninstall(force).await?;
        }
    }
    Ok(())
}
