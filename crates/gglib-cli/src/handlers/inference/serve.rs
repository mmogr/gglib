//! Serve command handler.
//!
//! Handles serving a GGUF model with llama-server.

use anyhow::Result;
use std::process::Stdio;

use crate::bootstrap::CliContext;
use crate::presentation::style;
use crate::shared_args::{ContextArgs, MtpArgs, SamplingArgs, ServeOptions};
use gglib_runtime::llama::{
    LlamaCommandBuilder, ensure_llama_initialized,
    resolve_llama_server, resolve_mtp_args,
};
use gglib_runtime::server_config::{ServerConfigOptions, resolve_context_size};

use super::shared::{
    log_command_execution, log_inference_info, log_mlock_info,
    resolve_inference_config,
};

/// Execute the serve command.
///
/// Starts llama-server with the specified model.
pub async fn execute(
    ctx: &CliContext,
    id: u32,
    context: ContextArgs,
    options: ServeOptions,
    sampling: SamplingArgs,
    mtp: MtpArgs,
    verbose: bool,
) -> Result<()> {
    // Ensure llama.cpp is installed
    ensure_llama_initialized().await?;

    // Resolve and validate llama-server binary path
    let llama_path = resolve_llama_server().map_err(|e| {
        anyhow::anyhow!(
            "{}\n\nTo install llama.cpp, run:\n  gglib config llama install",
            e
        )
    })?;

    // Look up the model using CliContext
    let model = ctx
        .app
        .models()
        .get_by_id(id as i64)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Model with ID {} not found", id))?;

    // Log model info
    style::print_info_banner("Info", "\u{2139}\u{fe0f}");
    eprintln!("  Using model: {} (ID: {})", model.name, model.id);
    eprintln!("  File: {}", model.file_path.display());

    // Handle context size
    let settings = ctx.app.settings().get().await?;
    let effective_ctx = resolve_context_size(&ServerConfigOptions {
        context_size: context.ctx_size.as_deref().and_then(|s| s.parse::<u64>().ok()),
        model_server_ctx: model.server_defaults.as_ref().and_then(|s| s.context_length),
        global_default_ctx: settings.default_context_size,
        ..Default::default()
    });
    eprintln!("  Context size: {} (resolved)", effective_ctx);
    log_mlock_info(context.mlock);

    // Resolve inference parameters using 3-level hierarchy
    let inference_config =
        resolve_inference_config(ctx, sampling.into_inference_config(), &model).await?;
    log_inference_info(&inference_config);

    // Handle Jinja flag
    if options.jinja {
        eprintln!("  Jinja templates: enabled");
    }

    // Resolve MTP speculative decoding
    let mtp = resolve_mtp_args(mtp.mtp_draft_n_max, mtp.mtp_draft_p_min, &model.tags);
    if mtp.enabled {
        eprintln!(
            "  MTP speculative decoding: enabled (n-max={}, p-min={:.2}, source={:?})",
            mtp.draft_n_max, mtp.draft_p_min, mtp.source
        );
    }

    eprintln!(
        "  Server will be available on http://localhost:{}",
        options.port
    );
    style::print_banner_close();

    // Build llama-server command
    let mut builder = LlamaCommandBuilder::new(&llama_path, &model.file_path)
        .context_size(effective_ctx)
        .mlock(context.mlock)
        .inference_config(inference_config)
        .arg_with_value("--port", options.port.to_string());

    if options.jinja {
        builder = builder.flag("--jinja");
    }

    if mtp.enabled {
        builder = builder
            .arg_with_value("--spec-type", "draft-mtp".to_string())
            .arg_with_value("--spec-draft-n-max", mtp.draft_n_max.to_string())
            .arg_with_value("--spec-draft-p-min", mtp.draft_p_min.to_string());
    }

    // Suppress llama-server's own INFO-level startup chatter unless --verbose.
    // -lv 1 = errors only; -lv 3 = INFO (llama-server default).
    let log_verbosity = if verbose { "3" } else { "1" };
    builder = builder.arg_with_value("-lv", log_verbosity.to_string());

    let mut cmd = builder.build();

    // Set up stdio to inherit from parent
    cmd.stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    log_command_execution(&cmd);
    eprintln!("  Starting server... (Press Ctrl+C to stop)");

    // Execute llama-server
    let status = cmd.status()?;

    if status.success() {
        eprintln!("  llama-server exited successfully");
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
