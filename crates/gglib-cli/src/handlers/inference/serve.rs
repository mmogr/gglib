//! Serve command handler.
//!
//! Handles serving a GGUF model with llama-server.

use anyhow::Result;
use std::process::Stdio;

use crate::bootstrap::CliContext;
use crate::shared_args::{ContextArgs, SamplingArgs};
use gglib_runtime::llama::{
    LlamaCommandBuilder, ensure_llama_initialized, resolve_context_size, resolve_llama_server,
};

use super::shared::{log_command_execution, log_context_info, log_inference_info, log_mlock_info, resolve_inference_config};

/// Execute the serve command.
///
/// Starts llama-server with the specified model.
pub async fn execute(
    ctx: &CliContext,
    id: u32,
    context: ContextArgs,
    jinja_flag: bool,
    port: u16,
    sampling: SamplingArgs,
) -> Result<()> {
    // Ensure llama.cpp is installed
    ensure_llama_initialized().await?;

    // Resolve and validate llama-server binary path
    let llama_path = resolve_llama_server().map_err(|e| {
        anyhow::anyhow!("{}\n\nTo install llama.cpp, run:\n  gglib config llama install", e)
    })?;

    // Look up the model using CliContext
    let model = ctx
        .app
        .models()
        .get_by_id(id as i64)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Model with ID {} not found", id))?;

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

    // Handle Jinja flag
    if jinja_flag {
        println!("Jinja templates: enabled");
    }

    println!("Server will be available on http://localhost:{}", port);

    // Build llama-server command
    let mut builder = LlamaCommandBuilder::new(&llama_path, &model.file_path)
        .context_resolution(context_resolution)
        .mlock(context.mlock)
        .inference_config(inference_config)
        .arg_with_value("--port", port.to_string());

    if jinja_flag {
        builder = builder.flag("--jinja");
    }

    let mut cmd = builder.build();

    // Set up stdio to inherit from parent
    cmd.stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    log_command_execution(&cmd);
    println!("Starting server... (Press Ctrl+C to stop)");

    // Execute llama-server
    let status = cmd.status()?;

    if status.success() {
        println!("llama-server exited successfully");
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "llama-server exited with code: {:?}",
            status.code()
        ))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_serve_handler_exists() {
        // Placeholder test to ensure module compiles
    }
}
