#![doc = include_str!("README.md")]
pub mod approve;
pub mod list;
pub mod render;
pub mod resume;
pub mod rewind;
pub mod run;
pub mod show;

// ─── Shared helpers ─────────────────────────────────────────────────────────

use std::sync::Arc;

use anyhow::{Context as _, Result, anyhow};

use gglib_core::ProcessHandle;
use gglib_core::domain::council::task_graph::HitlMode;
use gglib_core::request_pipeline;
use gglib_core::server_config::parse_ctx_size_flag;
use gglib_runtime::CouncilPorts;
use gglib_runtime::compose_council_ports;
use gglib_runtime::server_config::{ServerConfigOptions, build_server_config};

use gglib_core::domain::council::run::CouncilRunStatus;

use crate::bootstrap::CliContext;
use crate::presentation::style;

/// Map a [`CouncilRunStatus`] to a stable ANSI colour constant.
///
/// Shared by [`list`] and [`show`]; avoids duplicating the match arm.
pub(crate) fn status_color(status: &CouncilRunStatus) -> &'static str {
    match status {
        CouncilRunStatus::Running => style::INFO,
        CouncilRunStatus::AwaitingApproval => style::WARNING,
        CouncilRunStatus::Completed => style::SUCCESS,
        CouncilRunStatus::Failed => style::DANGER,
        CouncilRunStatus::Interrupted => style::DIM,
    }
}

pub(crate) fn parse_hitl_mode(hitl: Option<&str>) -> Result<HitlMode> {
    match hitl.unwrap_or("none") {
        "none" => Ok(HitlMode::None),
        "approve_plan" | "plan" => Ok(HitlMode::ApprovePlan),
        "approve_each_node" | "node" => Ok(HitlMode::ApproveEachNode),
        "approve_tools" | "tools" => Ok(HitlMode::ApproveTools),
        other => Err(anyhow!(
            "unknown HITL mode: '{other}'. Valid values: none, plan, node, tools"
        )),
    }
}

pub(crate) async fn init_session(
    ctx: &CliContext,
    port: Option<u16>,
    model: Option<String>,
    ctx_size: Option<String>,
    sampling: Option<gglib_core::domain::InferenceConfig>,
) -> Result<(CouncilPorts, Option<ProcessHandle>)> {
    let (resolved_port, handle) = resolve_port(ctx, port, &model, ctx_size).await?;

    if let Err(e) = ctx.mcp.initialize().await {
        tracing::warn!("MCP initialisation failed: {e}");
    }
    // Pre-warm lazy servers so they are ready before the council run.
    ctx.mcp.prewarm_lazy().await;

    let cwd = std::env::current_dir().ok();

    let model_context = request_pipeline::resolve(ctx.catalog.as_ref(), model.as_deref()).await;
    let ports = compose_council_ports(
        format!("http://127.0.0.1:{resolved_port}"),
        ctx.http_client.clone(),
        model,
        model_context,
        Arc::clone(&ctx.mcp),
        cwd,
        sampling,
        // No proxy dashboard in the CLI process — nowhere to report reuse.
        None,
    );
    Ok((ports, handle))
}

pub(crate) async fn resolve_port(
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

    style::print_info_banner("Orchestrate", "\u{1f916}");
    eprintln!("  Starting llama-server for '{}' \u{2026}", model_id.name);
    style::print_banner_close();

    let h = ctx
        .runner
        .start(server_config)
        .await
        .context("failed to start llama-server")?;
    Ok((h.port, Some(h)))
}

pub(crate) async fn stop_server(ctx: &CliContext, handle: &Option<ProcessHandle>) {
    if let Some(h) = handle
        && let Err(e) = ctx.runner.stop(h).await
    {
        tracing::warn!("failed to stop llama-server: {e}");
    }
}
