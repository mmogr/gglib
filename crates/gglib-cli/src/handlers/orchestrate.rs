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
    ApprovalKind, ORCHESTRATOR_EVENT_CHANNEL_CAPACITY, OrchestratorEvent,
};
use gglib_core::domain::orchestrator::task_graph::{HitlMode, NodeStatus};
use gglib_core::ports::{
    ApprovalDecision, OrchestratorApprovalRegistryPort, OrchestratorRepositoryPort,
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
#[allow(clippy::too_many_arguments)]
pub async fn execute_command(
    ctx: &CliContext,
    goal: Option<&str>,
    port: Option<u16>,
    model: Option<String>,
    ctx_size: Option<String>,
    max_replans: u32,
    hitl: Option<&str>,
    resume: Option<&str>,
) -> Result<()> {
    let hitl_mode = parse_hitl_mode(hitl)?;

    // ── Resume path ───────────────────────────────────────────────────────
    if let Some(run_id) = resume {
        return resume_run(ctx, run_id, port, model, ctx_size, max_replans, hitl_mode).await;
    }

    let goal = goal.ok_or_else(|| anyhow!("A goal is required when not using --resume"))?;

    let (ports, handle) = init_session(ctx, port, model, ctx_size).await?;

    let config = OrchestratorConfig {
        max_replans,
        hitl_mode,
        approval_registry: Some(
            Arc::clone(&ctx.approval_registry) as Arc<dyn OrchestratorApprovalRegistryPort>
        ),
        repository: Some(Arc::clone(&ctx.orchestrator_repo) as Arc<dyn OrchestratorRepositoryPort>),
        ..OrchestratorConfig::default()
    };

    let (tx, mut rx) = mpsc::channel::<OrchestratorEvent>(ORCHESTRATOR_EVENT_CHANNEL_CAPACITY);

    let approval_registry = Arc::clone(&ctx.approval_registry);
    let run_handle = {
        let llm = ports.llm;
        let tool_executor = ports.tool_executor;
        let goal_owned = goal.to_owned();
        tokio::spawn(async move { execute(&goal_owned, &[], llm, tool_executor, config, tx).await })
    };

    // ── Render events to terminal ─────────────────────────────────────────
    while let Some(event) = rx.recv().await {
        render_event(&event, &approval_registry).await;
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

fn parse_hitl_mode(hitl: Option<&str>) -> Result<HitlMode> {
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

/// Resume an interrupted or awaiting-approval run from the DB.
async fn resume_run(
    ctx: &CliContext,
    run_id: &str,
    port: Option<u16>,
    model: Option<String>,
    ctx_size: Option<String>,
    max_replans: u32,
    hitl_mode: HitlMode,
) -> Result<()> {
    let run = ctx
        .orchestrator_repo
        .get_run(run_id)
        .await
        .context("failed to load run from database")?
        .ok_or_else(|| anyhow!("run '{run_id}' not found"))?;

    let graph_json = run
        .graph_json
        .as_deref()
        .ok_or_else(|| anyhow!("run '{run_id}' has no saved graph — cannot resume"))?;

    let mut graph: gglib_core::domain::orchestrator::task_graph::TaskGraph =
        serde_json::from_str(graph_json).context("failed to deserialize saved graph")?;

    // Reset non-Done nodes so the executor re-runs them.
    for node in graph.nodes.values_mut() {
        if node.status != NodeStatus::Done {
            node.status = NodeStatus::Pending;
        }
    }

    let (ports, handle) = init_session(ctx, port, model, ctx_size).await?;

    let config = OrchestratorConfig {
        max_replans,
        hitl_mode,
        approval_registry: Some(
            Arc::clone(&ctx.approval_registry) as Arc<dyn OrchestratorApprovalRegistryPort>
        ),
        repository: Some(Arc::clone(&ctx.orchestrator_repo) as Arc<dyn OrchestratorRepositoryPort>),
        run_id: Some(run_id.to_owned()),
        graph_override: Some(graph),
        ..OrchestratorConfig::default()
    };

    eprintln!("{}  Resuming run {}{}", style::INFO, run_id, style::RESET);

    let (tx, mut rx) = mpsc::channel::<OrchestratorEvent>(ORCHESTRATOR_EVENT_CHANNEL_CAPACITY);
    let approval_registry = Arc::clone(&ctx.approval_registry);
    let run_handle = {
        let llm = ports.llm;
        let tool_executor = ports.tool_executor;
        let goal_owned = run.goal.clone();
        tokio::spawn(async move { execute(&goal_owned, &[], llm, tool_executor, config, tx).await })
    };

    while let Some(event) = rx.recv().await {
        render_event(&event, &approval_registry).await;
    }

    stop_server(ctx, &handle).await;

    match run_handle.await {
        Err(e) => Err(anyhow!("orchestrator task panicked: {e}")),
        Ok(Err(e)) => Err(anyhow!("{e}")),
        Ok(Ok(())) => Ok(()),
    }
}

/// Render a single [`OrchestratorEvent`] to the terminal.
///
/// For `AwaitingApproval` events, prompts the user interactively and
/// resolves the approval via the registry.
async fn render_event(
    event: &OrchestratorEvent,
    approval_registry: &Arc<gglib_app_services::OrchestratorApprovalRegistry>,
) {
    match event {
        OrchestratorEvent::PlanProposed { graph } => {
            style::print_info_banner("Orchestrate", "\u{1f5fa}\u{fe0f}");
            eprintln!(
                "  {}Plan proposed:{} {} node(s) for goal: {}",
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
        OrchestratorEvent::PlanRejected { reason } => {
            eprintln!(
                "{}  ✗ Plan rejected{}{}",
                style::DANGER,
                reason
                    .as_deref()
                    .map(|r| format!(": {r}"))
                    .unwrap_or_default(),
                style::RESET
            );
        }
        OrchestratorEvent::AwaitingApproval { approval_id, kind } => {
            prompt_and_resolve(approval_id, kind, approval_registry).await;
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
        OrchestratorEvent::NodeProgress { .. } | OrchestratorEvent::SynthesisProgress { .. } => {}
    }
}

/// Prompt the user for an approval decision and resolve it in the registry.
async fn prompt_and_resolve(
    approval_id: &str,
    kind: &ApprovalKind,
    registry: &Arc<gglib_app_services::OrchestratorApprovalRegistry>,
) {
    let description = match kind {
        ApprovalKind::Plan => "the proposed plan".to_owned(),
        ApprovalKind::Node { node_id } => format!("node '{node_id}'"),
        ApprovalKind::Tool { node_id, tool_name } => {
            format!("tool call '{tool_name}' in node '{node_id}'")
        }
    };

    eprintln!(
        "\n{}  ⏸  Awaiting approval for {description}{}",
        style::WARNING,
        style::RESET
    );
    eprintln!("  [y] approve  [n] reject  (Enter = approve)");
    eprint!("  Decision: ");

    let input = tokio::task::spawn_blocking(|| {
        let mut buf = String::new();
        let _ = std::io::stdin().read_line(&mut buf);
        buf.trim().to_lowercase()
    })
    .await
    .unwrap_or_default();

    let decision = match input.as_str() {
        "n" | "no" | "reject" => {
            eprint!("  Rejection reason (optional): ");
            let reason = tokio::task::spawn_blocking(|| {
                let mut buf = String::new();
                let _ = std::io::stdin().read_line(&mut buf);
                buf.trim().to_owned()
            })
            .await
            .unwrap_or_default();
            let reason = if reason.is_empty() {
                None
            } else {
                Some(reason)
            };
            ApprovalDecision::Reject(reason.unwrap_or_else(|| "rejected by user".to_owned()))
        }
        _ => ApprovalDecision::Approve,
    };

    registry.resolve(approval_id, decision);
}

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
