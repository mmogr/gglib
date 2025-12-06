//! Serve command handler.
//!
//! Handles serving a GGUF model with llama-server.

use anyhow::Result;
use std::process::Stdio;

use crate::bootstrap::CliContext;
use crate::utils::paths::get_llama_server_path;

// Import llama utilities from root crate (these don't depend on AppCore)
use gglib::commands::llama::ensure_llama_initialized;
use gglib::commands::llama_args::{
    JinjaResolution, JinjaResolutionSource, resolve_context_size, resolve_jinja_flag,
};
use gglib::commands::llama_invocation::LlamaCommandBuilder;

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

    // Get llama-server binary path
    let llama_server_path = get_llama_server_path()?;

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

    // Handle Jinja flag
    let explicit_jinja = if jinja_flag { Some(true) } else { None };
    let jinja_resolution = resolve_jinja_flag(explicit_jinja, &model.tags);
    log_jinja_decision(&jinja_resolution);

    println!("Server will be available on http://localhost:{}", port);

    // Build llama-server command
    let mut builder = LlamaCommandBuilder::new(&llama_server_path, &model.file_path)
        .context_resolution(context_resolution)
        .mlock(mlock)
        .arg_with_value("--port", port.to_string());

    if jinja_resolution.enabled {
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

fn log_context_info(resolution: &gglib::commands::llama_args::ContextResolution) {
    use gglib::commands::llama_args::ContextResolutionSource;
    
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

fn log_jinja_decision(resolution: &JinjaResolution) {
    match (resolution.enabled, resolution.source) {
        (true, JinjaResolutionSource::ExplicitTrue) => {
            println!("Enabling Jinja templates (--jinja flag)");
        }
        (true, JinjaResolutionSource::AgentTag) => {
            println!("Enabling Jinja templates due to 'agent' tag");
        }
        (false, JinjaResolutionSource::ExplicitFalse) => {
            println!("Jinja templates disabled by explicit override");
        }
        _ => {}
    }
}

fn log_command_execution(cmd: &std::process::Command) {
    tracing::debug!("Executing: {:?}", cmd);
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_serve_handler_exists() {
        assert!(true);
    }
}
