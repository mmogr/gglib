//! Chat command handler.
//!
//! Handles launching llama-cli for interactive chat with a model.

use anyhow::Result;
use std::process::Stdio;

use crate::bootstrap::CliContext;
use crate::shared_args::{ContextArgs, SamplingArgs};
use gglib_core::paths::llama_cli_path;
use gglib_runtime::llama::{
    LlamaCommandBuilder, ensure_llama_initialized, resolve_context_size,
};

use super::shared::{
    log_command_execution, log_context_info, log_inference_info, log_mlock_info,
    resolve_inference_config,
};

/// Arguments for the chat command.
#[derive(Debug, Clone)]
pub struct ChatArgs {
    pub identifier: String,
    pub context: ContextArgs,
    pub chat_template: Option<String>,
    pub chat_template_file: Option<String>,
    pub jinja: bool,
    pub system_prompt: Option<String>,
    pub multiline_input: bool,
    pub simple_io: bool,
    pub sampling: SamplingArgs,
    // Agentic mode fields
    pub agent: bool,
    pub port: Option<u16>,
    pub max_iterations: usize,
    pub tools: Vec<String>,
    pub tool_timeout_ms: Option<u64>,
    pub max_parallel: Option<usize>,
    /// Mirror of the global `--verbose` / `-v` flag for agentic mode rendering.
    pub verbose: bool,
    /// Optional model-name override for llama-server routing (agentic mode only).
    pub model: Option<String>,
}

/// Execute the chat command.
///
/// Launches llama-cli for interactive chat with the specified model.
///
/// # Arguments
///
/// * `ctx` - The CLI context providing access to AppCore
/// * `args` - Chat command arguments
pub async fn execute(ctx: &CliContext, args: ChatArgs) -> Result<()> {
    // Agentic mode: delegate entirely to the agent_chat handler.
    if args.agent {
        return crate::handlers::agent_chat::run(ctx, &args).await;
    }

    // Ensure llama.cpp is installed
    ensure_llama_initialized().await?;

    // Get llama-cli binary path
    let llama_cli_path = llama_cli_path()?;

    let ChatArgs {
        identifier,
        context,
        chat_template,
        chat_template_file,
        jinja,
        system_prompt,
        multiline_input,
        simple_io,
        sampling,
        // agent/port/max_iterations/tools already handled by the early-return above
        ..
    } = args;

    // Look up the model using CliContext
    let model = ctx.app.models().find_by_identifier(&identifier).await?;

    // Log model info
    println!("Using model: {} (ID: {})", model.name, model.id);
    println!("File: {}", model.file_path.display());

    // Handle context size
    let context_resolution = resolve_context_size(context.ctx_size, model.context_length)?;
    log_context_info(&context_resolution);
    log_mlock_info(context.mlock);

    // Resolve inference parameters using 3-level hierarchy
    let inference_config =
        resolve_inference_config(ctx, sampling.into_inference_config(), &model).await?;
    log_inference_info(&inference_config);

    // Build command using shared builder
    let mut cmd = LlamaCommandBuilder::new(&llama_cli_path, &model.file_path)
        .context_resolution(context_resolution)
        .mlock(context.mlock)
        .inference_config(inference_config)
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

    // Hand control to the user
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
        assert_send::<ChatArgs>();
    }
}
