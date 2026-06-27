#![doc = include_str!("README.md")]

#![doc = include_str!("README.md")]

// MIGRATION: content extracted to README.md — remove this //! block after review
// MIGRATION: content extracted to README.md — remove this //! block after review
//! `gglib council` subcommand group.
//!
//! Organised as a directory module; each subcommand lives in its own file.
//!
//! # Subcommands
//!
//! | File | Subcommand | Purpose |
//! |------|------------|---------|
//! | [`run`]    | `council run "<goal>"` | Plan and execute a new task graph |
//! | [`list`]   | `council list [--status]` | List past orchestrator runs |
//! | [`show`]   | `council show <id>` | Detailed event timeline for a run |
//! | [`resume`] | `council resume <id>` | Continue an interrupted run |
//! | [`rewind`] | `council rewind <id> --wave N` | Roll back to a previous wave and re-execute |
//!
//! # Shared helpers (private to the module)
//!
//! | Symbol | Purpose |
//! |--------|---------|
//! | [`parse_hitl_mode`] | Parse `--hitl` string → [`HitlMode`] |
//! | [`init_session`]    | Spin up (or reuse) a llama-server, compose [`CouncilPorts`] |
//! | [`resolve_port`]    | Select an explicit port or auto-allocate one |
//! | [`stop_server`]     | Gracefully stop an auto-started llama-server |
//!
//! # Internal architecture
//!
//! ```text
//!   stdin
//!     │
//!     ▼
//! presentation::input::spawn_input_router
//!     ├── /note <text>  ──►  NoteQueue  ──►  CouncilConfig  ──►  executor
//!     └── other line   ──►  mpsc::UnboundedReceiver<String>
//!                                │
//!                                ▼
//!                       approve::prompt_and_resolve
//!                       (tokio::time::timeout around recv())
//!                                │
//!                                ▼
//!                       CouncilApprovalRegistry::resolve
//! ```
//!
//! The event loop in `run` / `resume` / `rewind` receives [`CouncilEvent`]s
//! from the engine over a [`tokio::sync::mpsc`] channel and dispatches them
//! to [`render::render_event`], which either serialises them as JSONL
//! (`--json` mode) or renders them to the terminal with ANSI colour.
//!
//! [`CouncilEvent`]: gglib_core::domain::council::events::CouncilEvent

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

use gglib_core::domain::council::task_graph::HitlMode;
use gglib_core::{ProcessHandle, ServerConfig};
use gglib_runtime::CouncilPorts;
use gglib_runtime::compose_council_ports;
use gglib_runtime::llama::args::{
    ContextInput, resolve_context_size, resolve_jinja_flag, resolve_reasoning_format,
};

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

    let tags = match model.as_deref() {
        Some(name) => ctx.app.models().tags_for(name).await,
        None => Vec::new(),
    };
    let ports = compose_council_ports(
        format!("http://127.0.0.1:{resolved_port}"),
        ctx.http_client.clone(),
        model,
        tags,
        Arc::clone(&ctx.mcp),
        cwd,
        sampling,
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

    let mut server_config = ServerConfig::new(
        model_id.id,
        model_id.name.clone(),
        model_id.file_path.clone(),
        ctx.base_port,
    );

    let jinja = resolve_jinja_flag(None, &model_id.tags);
    if jinja.enabled {
        server_config = server_config.with_jinja();
    }
    let reasoning = resolve_reasoning_format(None, &model_id.tags);
    if let Some(format) = reasoning.format {
        server_config = server_config.with_reasoning_format(format);
    }

    let settings = ctx.app.settings().get().await.unwrap_or_default();
    let context_resolution = resolve_context_size(ContextInput {
        flag: ctx_size,
        model_context_length: model_id.context_length,
        settings_default: settings.default_context_size,
    })?;
    if let Some(ctx_val) = context_resolution.value {
        server_config = server_config.with_context_size(u64::from(ctx_val));
    }

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
