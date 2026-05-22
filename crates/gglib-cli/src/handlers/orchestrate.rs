//! CLI handler for `gglib orchestrate` — plan and execute a Director/Worker
//! task graph end-to-end.
//!
//! Each worker's text output is streamed to the terminal as it arrives,
//! prefixed with its node id so the user can follow multiple concurrent
//! workers.  Tool calls are rendered inline.  The final synthesis is printed
//! as a formatted answer.

use std::sync::Arc;

use anyhow::{Context as _, Result, anyhow};
use tokio::sync::mpsc;

use gglib_agent::orchestrator::{OrchestratorConfig, execute};
use gglib_core::domain::orchestrator::events::{
    ORCHESTRATOR_EVENT_CHANNEL_CAPACITY, OrchestratorEvent,
};
use gglib_core::{ProcessHandle, ServerConfig};
use gglib_runtime::CouncilPorts;
use gglib_runtime::compose_council_ports;
use gglib_runtime::llama::args::{
    ContextInput, resolve_context_size, resolve_jinja_flag, resolve_reasoning_format,
};

use crate::bootstrap::CliContext;
use crate::presentation::style;

// ─── Execute ────────────────────────────────────────────────────────────────

/// Run `gglib orchestrate "<goal>"`.
///
/// Resolves the LLM port, calls [`execute`] which handles planning +
/// worker execution + synthesis, and renders events to the terminal.
pub async fn execute_command(
    ctx: &CliContext,
    goal: &str,
    port: Option<u16>,
    model: Option<String>,
    ctx_size: Option<String>,
    max_replans: u32,
) -> Result<()> {
    let (ports, handle) = init_session(ctx, port, model, ctx_size).await?;

    let config = OrchestratorConfig {
        max_replans,
        ..OrchestratorConfig::default()
    };

    let (tx, mut rx) = mpsc::channel::<OrchestratorEvent>(ORCHESTRATOR_EVENT_CHANNEL_CAPACITY);

    let run_handle = {
        let llm = ports.llm;
        let tool_executor = ports.tool_executor;
        let goal_owned = goal.to_owned();
        tokio::spawn(async move { execute(&goal_owned, &[], llm, tool_executor, config, tx).await })
    };

    // ── Render events to terminal ─────────────────────────────────────────
    while let Some(event) = rx.recv().await {
        match event {
            OrchestratorEvent::PlanProposed { graph } => {
                style::print_info_banner("Orchestrate", "\u{1f5fa}\u{fe0f}");
                eprintln!(
                    "  {}Plan accepted:{} {} node(s) for goal: {}",
                    style::BOLD,
                    style::RESET,
                    graph.nodes.len(),
                    graph.goal
                );
                style::print_banner_close();
            }
            OrchestratorEvent::ReplanAttempt { attempt, reason } => {
                eprintln!(
                    "{}  ↻ Replanning (attempt {attempt}): {reason}{}",
                    style::DIM,
                    style::RESET
                );
            }
            OrchestratorEvent::PlanApproved => {
                eprintln!(
                    "{}  ✓ Plan approved — starting execution{}",
                    style::DIM,
                    style::RESET
                );
            }
            OrchestratorEvent::NodeStarted {
                node_id,
                goal: node_goal,
            } => {
                eprintln!(
                    "\n{}[{}]{} {}",
                    style::INFO,
                    node_id,
                    style::RESET,
                    node_goal
                );
            }
            OrchestratorEvent::NodeTextDelta { delta, .. } => {
                eprint!("{delta}");
            }
            OrchestratorEvent::NodeReasoningDelta { node_id, delta } => {
                eprint!("{}[{node_id}]<think> {delta}{}", style::DIM, style::RESET);
            }
            OrchestratorEvent::NodeToolCallStart {
                node_id,
                display_name,
                args_summary,
                ..
            } => {
                eprintln!(
                    "\n{}[{node_id}] ⚙ {}  {}{}",
                    style::DIM,
                    display_name,
                    args_summary.as_deref().unwrap_or(""),
                    style::RESET
                );
            }
            OrchestratorEvent::NodeToolCallComplete {
                node_id,
                display_name,
                duration_display,
                ..
            } => {
                eprintln!(
                    "{}[{node_id}] ✓ {}  {}{}",
                    style::DIM,
                    display_name,
                    duration_display,
                    style::RESET
                );
            }
            OrchestratorEvent::NodeSystemWarning {
                node_id, message, ..
            } => {
                eprintln!(
                    "{}[{node_id}] ⚠ {}{}",
                    style::WARNING,
                    message,
                    style::RESET
                );
            }
            OrchestratorEvent::NodeCompacting { node_id } => {
                eprintln!(
                    "\n{}[{node_id}] compacting output…{}",
                    style::DIM,
                    style::RESET
                );
            }
            OrchestratorEvent::NodeComplete { node_id, .. } => {
                eprintln!("{}[{node_id}] ✓ complete{}", style::SUCCESS, style::RESET);
            }
            OrchestratorEvent::NodeFailed { node_id, error } => {
                eprintln!(
                    "{}[{node_id}] ✗ failed: {error}{}",
                    style::DANGER,
                    style::RESET
                );
            }
            OrchestratorEvent::SynthesisStart => {
                eprintln!("\n{}─── Synthesis ───{}", style::BOLD, style::RESET);
            }
            OrchestratorEvent::SynthesisTextDelta { delta } => {
                eprint!("{delta}");
            }
            OrchestratorEvent::SynthesisComplete { .. } => {
                eprintln!();
            }
            OrchestratorEvent::OrchestratorComplete { answer } => {
                eprintln!("\n{}─── Final Answer ───{}", style::BOLD, style::RESET);
                println!("{answer}");
            }
            OrchestratorEvent::OrchestratorError { message } => {
                eprintln!("{}Error: {message}{}", style::DANGER, style::RESET);
            }
            // Phase D events (HITL) — not used yet.
            OrchestratorEvent::AwaitingApproval { .. }
            | OrchestratorEvent::PlanRejected { .. }
            | OrchestratorEvent::NodeProgress { .. }
            | OrchestratorEvent::SynthesisProgress { .. } => {}
        }
    }

    stop_server(ctx, &handle).await;

    // Propagate errors from the executor task.
    match run_handle.await {
        Err(e) => Err(anyhow!("orchestrator task panicked: {e}")),
        Ok(Err(e)) => Err(anyhow!("{e}")),
        Ok(Ok(())) => Ok(()),
    }
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

async fn stop_server(ctx: &CliContext, handle: &Option<ProcessHandle>) {
    if let Some(h) = handle
        && let Err(e) = ctx.runner.stop(h).await
    {
        tracing::warn!("failed to stop llama-server: {e}");
    }
}
