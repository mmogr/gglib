//! `gglib council rewind <run-id> --wave N` — rewind a run to a previous
//! wave and re-execute from that point.
//!
//! Mirrors the logic of the Axum `POST /api/council/runs/{id}/rewind`
//! endpoint but runs entirely in-process without an HTTP server:
//!
//! 1. Load the run and its full event log from the repository.
//! 2. Refuse to rewind runs that are currently active (Running /
//!    AwaitingApproval).
//! 3. Identify nodes that completed in waves **after** `wave` and reset them
//!    to `Pending`.
//! 4. Truncate the event log at `wave`.
//! 5. Persist the updated graph.
//! 6. Optionally pre-seed the `NoteQueue` with the `--note` text so the
//!    steering LLM can course-correct at the first wave boundary.
//! 7. Re-execute the engine and stream events to the terminal.

use std::collections::HashSet;
use std::sync::Arc;

use anyhow::{Context as _, Result, anyhow};
use tokio::sync::mpsc;

use gglib_agent::council::{CouncilConfig, NoteQueue, execute as engine_execute};
use gglib_core::domain::council::events::COUNCIL_EVENT_CHANNEL_CAPACITY;
use gglib_core::domain::council::run::CouncilRunStatus;
use gglib_core::domain::council::task_graph::NodeStatus;
use gglib_core::ports::{CouncilApprovalRegistryPort, CouncilRepositoryPort};

use crate::bootstrap::CliContext;
use crate::presentation::input::spawn_input_router;
use crate::presentation::style;

use super::render::{RenderState, render_event};
use super::{approve, init_session, stop_server};

/// Rewind run `run_id` to `wave` and re-execute from that point.
#[allow(clippy::too_many_arguments)]
pub async fn execute(
    ctx: &CliContext,
    run_id: &str,
    wave: u32,
    note: Option<&str>,
    port: Option<u16>,
    model: Option<String>,
    ctx_size: Option<String>,
) -> Result<()> {
    // ── 1. Load run ──────────────────────────────────────────────────────────
    let run = ctx
        .council_repo
        .get_run(run_id)
        .await
        .context("failed to load run from database")?
        .ok_or_else(|| anyhow!("run '{run_id}' not found"))?;

    // ── 2. Guard: refuse active runs ─────────────────────────────────────────
    match run.status {
        CouncilRunStatus::Running | CouncilRunStatus::AwaitingApproval => {
            anyhow::bail!(
                "run '{run_id}' is currently active ({}) — cancel it before rewinding",
                run.status
            );
        }
        _ => {}
    }

    let graph_json = run
        .graph_json
        .as_deref()
        .ok_or_else(|| anyhow!("run '{run_id}' has no saved graph"))?;

    let mut graph: gglib_core::domain::council::task_graph::TaskGraph =
        serde_json::from_str(graph_json).context("failed to deserialise graph")?;

    // ── 3. Identify nodes completed after the target wave ────────────────────
    let events = ctx
        .council_repo
        .list_events(run_id)
        .await
        .context("failed to load events")?;

    let nodes_to_reset: HashSet<String> = events
        .iter()
        .filter(|ev| ev.wave_index > wave)
        .filter_map(|ev| {
            let v: serde_json::Value = serde_json::from_str(&ev.event_json).ok()?;
            if v.get("type")?.as_str()? == "node_complete" {
                v.get("node_id")?.as_str().map(String::from)
            } else {
                None
            }
        })
        .collect();

    for (id, node) in graph.nodes.iter_mut() {
        if nodes_to_reset.contains(id.0.as_str()) {
            node.status = NodeStatus::Pending;
            node.output = None;
            node.error = None;
        }
    }

    // ── 4. Truncate event log ────────────────────────────────────────────────
    ctx.council_repo
        .truncate_events_after_wave(run_id, wave)
        .await
        .context("failed to truncate event log")?;

    // ── 5. Persist updated graph ─────────────────────────────────────────────
    if let Ok(json) = serde_json::to_string(&graph) {
        let _ = ctx.council_repo.update_graph(run_id, &json).await;
    }

    // ── 6. Build NoteQueue, optionally pre-seed with --note ──────────────────
    let initial_notes: Vec<String> = note
        .filter(|n| !n.trim().is_empty())
        .map(|n| vec![n.trim().to_owned()])
        .unwrap_or_default();
    let note_queue: NoteQueue = Arc::new(tokio::sync::Mutex::new(initial_notes));
    let mut input_rx = spawn_input_router(Arc::clone(&note_queue));

    // ── 7. Re-execute ────────────────────────────────────────────────────────
    eprintln!(
        "{}  Rewinding run {} to wave {}{}",
        style::INFO,
        run_id,
        wave,
        style::RESET
    );

    // HITL mode comes from the persisted run record so the re-execution
    // honours the same gate policy as the original run.
    let hitl_mode = run.hitl_mode.clone();
    let approve_opts = approve::ApproveOpts::default();

    let (ports, handle) = init_session(ctx, port, model, ctx_size).await?;

    let config = CouncilConfig {
        hitl_mode,
        approval_registry: Some(
            Arc::clone(&ctx.approval_registry) as Arc<dyn CouncilApprovalRegistryPort>
        ),
        repository: Some(Arc::clone(&ctx.council_repo) as Arc<dyn CouncilRepositoryPort>),
        run_id: Some(run_id.to_owned()),
        graph_override: Some(graph),
        note_queue: Some(note_queue),
        rewind_to_wave: Some(wave),
        ..CouncilConfig::default()
    };

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

    let mut state = RenderState::new();
    while let Some(event) = rx.recv().await {
        render_event(
            &event,
            &approval_registry,
            &mut state,
            &approve_opts,
            false, // json_mode not supported for rewind in this phase
            &mut input_rx,
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
