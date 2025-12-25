//! Chat command handler.
//!
//! Handles launching llama-cli for interactive chat with a model.

use anyhow::Result;
use std::process::Stdio;

use crate::bootstrap::CliContext;
use gglib_core::paths::llama_cli_path;
use gglib_runtime::llama::{
    ContextResolution, ContextResolutionSource, LlamaCommandBuilder, ensure_llama_initialized,
    resolve_context_size,
};

/// Arguments for the chat command.
#[derive(Debug, Clone)]
pub struct ChatArgs {
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

/// Execute the chat command.
///
/// Launches llama-cli for interactive chat with the specified model.
///
/// # Arguments
///
/// * `ctx` - The CLI context providing access to AppCore
/// * `args` - Chat command arguments
pub async fn execute(ctx: &CliContext, args: ChatArgs) -> Result<()> {
    // Ensure llama.cpp is installed
    ensure_llama_initialized().await?;

    // Get llama-cli binary path
    let llama_cli_path = llama_cli_path()?;

    let ChatArgs {
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

    // Look up the model using CliContext
    let model = ctx.app().models().find_by_identifier(&identifier).await?;

    // Log model info
    println!("Using model: {} (ID: {})", model.name, model.id);
    println!("File: {}", model.file_path.display());

    // Handle context size
    let context_resolution = resolve_context_size(ctx_size, model.context_length)?;
    log_context_info(&context_resolution);
    log_mlock_info(mlock);

    // Build command using shared builder
    let mut cmd = LlamaCommandBuilder::new(&llama_cli_path, &model.file_path)
        .context_resolution(context_resolution)
        .mlock(mlock)
        .build();

    // Add chat-specific flags - skip --jinja for models with strict template constraints
    let should_enable_jinja = if jinja {
        if !model.capabilities.supports_system_role() || model.capabilities.requires_strict_turns()
        {
            println!("⚠️  Skipping --jinja flag due to strict template constraints.");
            false
        } else {
            true
        }
    } else {
        false
    };

    if should_enable_jinja {
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

fn log_context_info(resolution: &ContextResolution) {
    match (&resolution.value, &resolution.source) {
        (Some(size), ContextResolutionSource::ExplicitFlag) => {
            println!("Context size: {} (explicit)", size);
        }
        (Some(size), ContextResolutionSource::ModelMetadata) => {
            println!("Context size: {} (from model metadata)", size);
        }
        (None, ContextResolutionSource::NotSpecified) => {
            println!("Context size: default (not specified)");
        }
        (None, ContextResolutionSource::MaxRequestedMissing) => {
            println!("Context size: max requested but not in metadata");
        }
        _ => {}
    }
}

fn log_mlock_info(mlock: bool) {
    if mlock {
        println!("Memory lock: enabled");
    }
}

fn log_command_execution(cmd: &std::process::Command) {
    tracing::debug!("Executing: {:?}", cmd);
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
