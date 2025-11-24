//! Chat command implementation that launches llama-cli directly.

use crate::commands::common::resolve_context_size;
use crate::commands::llama::ensure_llama_initialized;
use crate::commands::llama_invocation::{
    LlamaCommandBuilder, log_command_execution, log_context_info, log_mlock_info, log_model_info,
    resolve_model_for_invocation,
};
use crate::services::database;
use crate::utils::paths::get_llama_cli_path;
use anyhow::Result;
use std::process::Stdio;

/// Arguments supported by the `gglib chat` command.
#[derive(Debug, Clone)]
pub struct ChatCommandArgs {
    pub identifier: String,
    pub ctx_size: Option<String>,
    pub mlock: bool,
    pub chat_template: Option<String>,
    pub chat_template_file: Option<String>,
    pub jinja: bool,
    pub system_prompt: Option<String>,
    pub multiline_input: bool,
    pub simple_io: bool,
}

/// Launch llama-cli interactively for the requested model.
pub async fn handle_chat(args: ChatCommandArgs) -> Result<()> {
    // Ensure llama.cpp is installed
    ensure_llama_initialized().await?;

    // Resolve llama-cli binary
    let llama_cli_path = get_llama_cli_path()?;

    let pool = database::setup_database().await?;
    let ChatCommandArgs {
        identifier,
        ctx_size,
        mlock,
        chat_template,
        chat_template_file,
        jinja,
        system_prompt,
        multiline_input,
        simple_io,
    } = args;

    // Use shared model resolution helper
    let resolved = resolve_model_for_invocation(&pool, &identifier).await?;
    let model = resolved.model;

    log_model_info(&model, "llama-cli");

    let context_resolution = resolve_context_size(ctx_size, model.context_length)?;
    log_context_info(&context_resolution);
    log_mlock_info(mlock);

    // Build command using shared builder
    let mut cmd = LlamaCommandBuilder::new(&llama_cli_path, &model.file_path)
        .context_resolution(context_resolution)
        .mlock(mlock)
        .flag("--interactive-first")
        .build();

    // Add chat-specific flags
    if jinja {
        cmd.arg("--jinja");
    }

    if let Some(template) = chat_template {
        cmd.arg("--chat-template").arg(template);
    }

    if let Some(template_file) = chat_template_file {
        cmd.arg("--chat-template-file").arg(template_file);
    }

    if let Some(prompt) = system_prompt {
        cmd.arg("-sys").arg(prompt);
    }

    if multiline_input {
        cmd.arg("--multiline-input");
    }

    if simple_io {
        cmd.arg("--simple-io");
    }

    // Hand control to the user.
    cmd.stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    log_command_execution(&cmd);
    println!("Chat session ready. Press Ctrl+C to exit.");

    let status = cmd.status()?;
    if status.success() {
        println!("llama-cli exited successfully");
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "llama-cli exited with code: {:?}",
            status.code()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_args_struct_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<ChatCommandArgs>();
    }
}
