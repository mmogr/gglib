//! llama.cpp management command handler.
//!
//! Routes each `LlamaCommand` variant to what carries it out. Installing,
//! which is also updating, is `engine`'s, shared with stable-diffusion.cpp;
//! `status` is printed by `gglib_runtime::llama`; and uninstalling is
//! `engine`'s too.

use anyhow::Result;

use crate::bootstrap::CliContext;
use crate::llama_commands::LlamaCommand;
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
            engine::uninstall(ctx, RuntimeKind::Llama, force).await?;
        }
        LlamaCommand::Detect { json } => {
            llama_detect::execute(json)?;
        }
    }
    Ok(())
}
