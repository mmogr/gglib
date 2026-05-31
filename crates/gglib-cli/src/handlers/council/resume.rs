//! `gglib council resume <run-id>` — continue an interrupted or
//! awaiting-approval run from its saved graph.

use std::collections::HashSet;
use std::sync::Arc;

use anyhow::{Context as _, Result, anyhow};
use tokio::sync::mpsc;

use gglib_agent::council::{CouncilConfig, NoteQueue, execute as engine_execute};
use gglib_core::domain::council::events::COUNCIL_EVENT_CHANNEL_CAPACITY;
use gglib_core::domain::council::task_graph::NodeStatus;
use gglib_core::ports::{CouncilApprovalRegistryPort, CouncilRepositoryPort};

use crate::bootstrap::CliContext;
use crate::presentation::input::spawn_input_router;
use crate::presentation::style;

use super::render::render_event;
use super::{approve, init_session, parse_hitl_mode, stop_server};

/// Resume run `run_id` from its last saved graph.
#[allow(clippy::too_many_arguments)]
pub async fn execute(
    ctx: &CliContext,
    run_id: &str,
    port: Option<u16>,
    model: Option<String>,
    ctx_size: Option<String>,
    max_replans: u32,
    hitl: Option<&str>,
    approval_timeout: Option<u64>,
    approval_timeout_action: &str,
    json_mode: bool,
) -> Result<()> {
    if json_mode && hitl.is_some_and(|h| h != "none") {
        anyhow::bail!("--json output requires --hitl none");
    }
    let hitl_mode = parse_hitl_mode(hitl)?;
    let timeout_action = approve::parse_timeout_action(approval_timeout_action)?;
    let approve_opts = approve::ApproveOpts {
        timeout_secs: approval_timeout,
        timeout_action,
    };

    let run = ctx
        .council_repo
        .get_run(run_id)
        .await
        .context("failed to load run from database")?
        .ok_or_else(|| anyhow!("run '{run_id}' not found"))?;

    let graph_json = run
        .graph_json
        .as_deref()
        .ok_or_else(|| anyhow!("run '{run_id}' has no saved graph — cannot resume"))?;

    let mut graph: gglib_core::domain::council::task_graph::TaskGraph =
        serde_json::from_str(graph_json).context("failed to deserialise saved graph")?;

    // Reset non-Done nodes so the executor re-runs them.
    for node in graph.nodes.values_mut() {
        if node.status != NodeStatus::Done {
            node.status = NodeStatus::Pending;
        }
    }

    let (ports, handle) = init_session(ctx, port, model, ctx_size).await?;

    let note_queue: NoteQueue = Arc::new(tokio::sync::Mutex::new(vec![]));
    let mut input_rx = spawn_input_router(Arc::clone(&note_queue));

    let config = CouncilConfig {
        max_replans,
        hitl_mode,
        approval_registry: Some(
            Arc::clone(&ctx.approval_registry) as Arc<dyn CouncilApprovalRegistryPort>
        ),
        repository: Some(Arc::clone(&ctx.council_repo) as Arc<dyn CouncilRepositoryPort>),
        run_id: Some(run_id.to_owned()),
        graph_override: Some(graph),
        note_queue: Some(note_queue),
        ..CouncilConfig::default()
    };

    eprintln!("{}  Resuming run {}{}", style::INFO, run_id, style::RESET);
    let (tx, mut rx) = mpsc::channel(COUNCIL_EVENT_CHANNEL_CAPACITY);
    let approval_registry = Arc::clone(&ctx.approval_registry);
    let run_handle = {
        let llm = ports.llm;
        let tool_executor = ports.tool_executor;
        let goal_owned = run.goal.clone();
        tokio::spawn(async move {
            engine_execute(&goal_owned, &[], llm, tool_executor, config, tx).await
        })
    };

    let mut last_graph = None;
    let mut thinking_nodes = HashSet::new();
    while let Some(event) = rx.recv().await {
        render_event(
            &event,
            &approval_registry,
            &mut last_graph,
            &approve_opts,
            json_mode,
            &mut input_rx,
            &mut thinking_nodes,
        )
        .await;
    }

    stop_server(ctx, &handle).await;

    match run_handle.await {
        Err(e) => Err(anyhow!("orchestrator task panicked: {e}")),
        Ok(Err(e)) => Err(anyhow!("{e}")),
        Ok(Ok(())) => Ok(()),
    }
}
