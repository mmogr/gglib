//! Serve command handler.
//!
//! Handles serving a GGUF model with llama-server.

use anyhow::Result;
use std::process::Stdio;

use crate::bootstrap::CliContext;
use gglib_runtime::llama::{
    ContextResolution, ContextResolutionSource, LlamaCommandBuilder, ensure_llama_initialized,
    resolve_context_size, resolve_llama_server,
};

/// Execute the serve command.
///
/// Starts llama-server with the specified model.
///
/// # Arguments
///
/// * `ctx` - The CLI context providing access to AppCore
/// * `id` - ID of the model to serve
/// * `ctx_size` - Optional context size ("max" for auto-detect, or number)
/// * `mlock` - Whether to enable memory lock
/// * `jinja_flag` - Whether to enable Jinja templates
/// * `port` - Port to serve on
pub async fn execute(
    ctx: &CliContext,
    id: u32,
    ctx_size: Option<String>,
    mlock: bool,
    jinja_flag: bool,
    port: u16,
) -> Result<()> {
    // Ensure llama.cpp is installed
    ensure_llama_initialized().await?;

    // Resolve and validate llama-server binary path
    // This will provide a clear, actionable error if the binary is missing
    let llama_path = resolve_llama_server().map_err(|e| {
        // Convert to anyhow error with helpful message
        anyhow::anyhow!("{}\n\nTo install llama.cpp, run:\n  gglib llama install", e)
    })?;

    // Look up the model using CliContext
    let model = ctx
        .app()
        .models()
        .get_by_id(id as i64)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Model with ID {} not found", id))?;

    // Log model info
    println!("Using model: {} (ID: {})", model.name, model.id);
    println!("File: {}", model.file_path.display());

    // Handle context size
    let context_resolution = resolve_context_size(ctx_size, model.context_length)?;
    log_context_info(&context_resolution);
    log_mlock_info(mlock);

    // Handle Jinja flag - skip for models with strict template constraints
    let should_enable_jinja = if jinja_flag {
        if !model.capabilities.supports_system_role() || model.capabilities.requires_strict_turns()
        {
            println!("⚠️  Skipping --jinja flag due to strict template constraints.");
            println!("   Chat proxy will handle message transformations instead.");
            false
        } else {
            println!("Jinja templates: enabled");
            true
        }
    } else {
        false
    };

    println!("Server will be available on http://localhost:{}", port);

    // Build llama-server command
    let mut builder = LlamaCommandBuilder::new(&llama_path, &model.file_path)
        .context_resolution(context_resolution)
        .mlock(mlock)
        .arg_with_value("--port", port.to_string());

    if should_enable_jinja {
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
    #[test]
    fn test_serve_handler_exists() {
        // Placeholder test to ensure module compiles
    }
}
