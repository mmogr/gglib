//! Chat command handler.
//!
//! Handles launching llama-cli for interactive chat with a model.

use anyhow::Result;
use std::process::Stdio;

use crate::bootstrap::CliContext;
use gglib_core::domain::InferenceConfig;
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
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub top_k: Option<i32>,
    pub max_tokens: Option<u32>,
    pub repeat_penalty: Option<f32>,
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
        temperature,
        top_p,
        top_k,
        max_tokens,
        repeat_penalty,
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

    // Resolve inference parameters using hierarchy: CLI args → Model → Global → Hardcoded
    let mut inference_config = InferenceConfig {
        temperature,
        top_p,
        top_k,
        max_tokens,
        repeat_penalty,
    };

    // Apply model defaults
    if let Some(ref model_defaults) = model.inference_defaults {
        inference_config.merge_with(model_defaults);
    }

    // Apply global defaults
    let settings = ctx.app().settings().get().await?;
    if let Some(ref global_defaults) = settings.inference_defaults {
        inference_config.merge_with(global_defaults);
    }

    // Apply hardcoded defaults
    inference_config.merge_with(&InferenceConfig::with_hardcoded_defaults());

    // Log resolved inference parameters
    log_inference_info(&inference_config);

    // Build command using shared builder
    let mut cmd = LlamaCommandBuilder::new(&llama_cli_path, &model.file_path)
        .context_resolution(context_resolution)
        .mlock(mlock)
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

fn log_inference_info(config: &InferenceConfig) {
    println!("Inference parameters:");
    if let Some(temp) = config.temperature {
        println!("  Temperature: {}", temp);
    }
    if let Some(top_p) = config.top_p {
        println!("  Top-p: {}", top_p);
    }
    if let Some(top_k) = config.top_k {
        println!("  Top-k: {}", top_k);
    }
    if let Some(max_tokens) = config.max_tokens {
        println!("  Max tokens: {}", max_tokens);
    }
    if let Some(repeat_penalty) = config.repeat_penalty {
        println!("  Repeat penalty: {}", repeat_penalty);
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
