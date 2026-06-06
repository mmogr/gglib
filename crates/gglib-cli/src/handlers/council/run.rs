//! `gglib council run "<goal>"` — plan and execute a task graph.

use std::sync::Arc;

use anyhow::{Result, anyhow};
use tokio::sync::mpsc;

use gglib_agent::council::{CouncilConfig, NoteQueue, execute as engine_execute};
use gglib_core::domain::agent::AgentConfig;
use gglib_core::domain::council::events::COUNCIL_EVENT_CHANNEL_CAPACITY;
use gglib_core::ports::{CouncilApprovalRegistryPort, CouncilRepositoryPort};

use crate::bootstrap::CliContext;
use crate::handlers::inference::shared::resolve_max_iterations;
use crate::presentation::input::spawn_input_router;
use crate::shared_args::SamplingArgs;

use super::render::{RenderState, render_event};
use super::{approve, init_session, parse_hitl_mode, stop_server};

/// Plan and execute a task graph for `goal`.
#[allow(clippy::too_many_arguments)]
pub async fn execute(
    ctx: &CliContext,
    goal: &str,
    port: Option<u16>,
    model: Option<String>,
    ctx_size: Option<String>,
    max_replans: u32,
    max_iterations: Option<usize>,
    sampling: SamplingArgs,
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

    let (ports, handle) = init_session(ctx, port, model, ctx_size, {
        let cfg = sampling.into_inference_config();
        if cfg == Default::default() { None } else { Some(cfg) }
    }).await?;

    let settings = ctx
        .app
        .settings()
        .get()
        .await
        .map_err(|e| anyhow!("failed to load settings: {e}"))?;
    let resolved_max_iterations = resolve_max_iterations(max_iterations, &settings);
    let worker_agent_config =
        AgentConfig::from_user_params(Some(resolved_max_iterations), None, None, None, None)
            .map_err(|e| anyhow!("invalid agent config: {e}"))?;

    let note_queue: NoteQueue = Arc::new(tokio::sync::Mutex::new(vec![]));
    let mut input_rx = spawn_input_router(Arc::clone(&note_queue));

    let config = CouncilConfig {
        max_replans,
        hitl_mode,
        worker_agent_config,
        approval_registry: Some(
            Arc::clone(&ctx.approval_registry) as Arc<dyn CouncilApprovalRegistryPort>
        ),
        repository: Some(Arc::clone(&ctx.council_repo) as Arc<dyn CouncilRepositoryPort>),
        note_queue: Some(note_queue),
        ..CouncilConfig::default()
    };

    let (tx, mut rx) = mpsc::channel(COUNCIL_EVENT_CHANNEL_CAPACITY);
    let approval_registry = Arc::clone(&ctx.approval_registry);
    let run_handle = {
        let llm = ports.llm;
        let tool_executor = ports.tool_executor;
        let goal_owned = goal.to_owned();
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
            json_mode,
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
