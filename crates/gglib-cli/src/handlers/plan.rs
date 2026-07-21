//! CLI handler for `gglib plan` — decompose a goal into a task-graph DAG.
//!
//! Calls the director agent, then renders the resulting [`TaskGraph`] as:
//! - An indented tree (stdout) showing each node and its dependencies.
//! - A Mermaid diagram (stdout) ready to paste into documentation.
//!
//! A llama-server is started automatically when `--port` is omitted.

use std::sync::Arc;

use anyhow::{Context as _, Result, anyhow};

use gglib_agent::council::plan;
use gglib_core::ProcessHandle;
use gglib_core::domain::council::task_graph::HitlMode;
use gglib_core::request_pipeline;
use gglib_core::server_config::parse_ctx_size_flag;
use gglib_runtime::CouncilPorts;
use gglib_runtime::compose_council_ports;
use gglib_runtime::server_config::{ServerConfigOptions, build_server_config};

use crate::bootstrap::CliContext;
use crate::presentation::{dag, style};

// ─── Execute ────────────────────────────────────────────────────────────────

/// Run `gglib plan "<goal>"`.
///
/// Resolves the LLM port, calls the director, then prints an indented tree
/// and Mermaid diagram to stdout.
pub async fn execute(
    ctx: &CliContext,
    goal: &str,
    port: Option<u16>,
    model: Option<String>,
    ctx_size: Option<String>,
    max_replans: u32,
) -> Result<()> {
    let (ports, handle) = init_session(ctx, port, model, ctx_size).await?;

    eprintln!("{}  Planning: {}{}…", style::DIM, style::RESET, goal);

    let res = plan(goal, &[], ports.llm, HitlMode::None, max_replans, None).await;

    stop_server(ctx, &handle).await;

    let graph = res.map_err(|e| anyhow!("{e}"))?;

    dag::render_tree(&graph, &mut std::io::stdout());
    println!();
    dag::render_mermaid(&graph, &mut std::io::stdout());

    Ok(())
}

// ─── Helpers ────────────────────────────────────────────────────────────────

async fn init_session(
    ctx: &CliContext,
    port: Option<u16>,
    model: Option<String>,
    ctx_size: Option<String>,
) -> Result<(CouncilPorts, Option<ProcessHandle>)> {
    let (resolved_port, handle) = resolve_port(ctx, port, &model, ctx_size).await?;

    if let Err(e) = ctx.mcp.initialize().await {
        tracing::warn!("MCP initialisation failed: {e}");
    }

    let cwd = std::env::current_dir().ok();

    let model_context = request_pipeline::resolve(ctx.catalog.as_ref(), model.as_deref()).await;
    let ports = compose_council_ports(
        format!("http://127.0.0.1:{resolved_port}"),
        ctx.http_client.clone(),
        model,
        model_context,
        Arc::clone(&ctx.mcp),
        cwd,
        None,
    );
    Ok((ports, handle))
}

async fn resolve_port(
    ctx: &CliContext,
    port: Option<u16>,
    model_arg: &Option<String>,
    ctx_size: Option<String>,
) -> Result<(u16, Option<ProcessHandle>)> {
    if let Some(p) = port {
        return Ok((p, None));
    }

    let model_id = if let Some(name) = model_arg {
        ctx.app
            .models()
            .find_by_identifier(name)
            .await
            .context("failed to look up model")?
    } else {
        let settings = ctx
            .app
            .settings()
            .get()
            .await
            .map_err(|e| anyhow!("failed to load settings: {e}"))?;
        let default_id = settings.default_model_id.ok_or_else(|| {
            anyhow!(
                "No model specified and no default model set.\n\
                 Use --model <name> or set a default:\n  \
                 gglib config default <name>"
            )
        })?;
        ctx.app
            .models()
            .get_by_id(default_id)
            .await
            .map_err(|e| anyhow!("failed to load default model: {e}"))?
            .ok_or_else(|| anyhow!("default model (ID: {default_id}) not found"))?
    };

    let settings = ctx.app.settings().get().await.unwrap_or_default();
    // Shape-validate the raw flag first, resolve against the model's GGUF
    // context length now that `model_id` is available (`--ctx-size max`).
    let ctx_arg = parse_ctx_size_flag(ctx_size.as_deref())?;
    let server_config = build_server_config(
        model_id.id,
        model_id.name.clone(),
        model_id.file_path.clone(),
        ctx.base_port,
        &model_id.tags,
        ServerConfigOptions {
            context_size: ctx_arg.and_then(|arg| arg.resolve(model_id.context_length)),
            model_server_ctx: model_id
                .server_defaults
                .as_ref()
                .and_then(|s| s.context_length),
            global_default_ctx: settings.default_context_size,
            ..Default::default()
        },
    );

    style::print_info_banner("Plan", "\u{1f5fa}\u{fe0f}");
    eprintln!("  Starting llama-server for '{}' \u{2026}", model_id.name);
    style::print_banner_close();

    let h = ctx
        .runner
        .start(server_config)
        .await
        .context("failed to start llama-server")?;
    Ok((h.port, Some(h)))
}

async fn stop_server(ctx: &CliContext, handle: &Option<ProcessHandle>) {
    if let Some(h) = handle
        && let Err(e) = ctx.runner.stop(h).await
    {
        tracing::warn!("failed to stop llama-server: {e}");
    }
}
