//! `gglib council run "<goal>"` — plan and execute a task graph.

use std::sync::Arc;

use anyhow::{Result, anyhow};
use tokio::sync::mpsc;

use gglib_agent::council::{CouncilConfig, execute as engine_execute};
use gglib_core::domain::council::events::COUNCIL_EVENT_CHANNEL_CAPACITY;
use gglib_core::ports::{CouncilApprovalRegistryPort, CouncilRepositoryPort};

use crate::bootstrap::CliContext;

use super::{init_session, parse_hitl_mode, stop_server};
use super::render::render_event;

/// Plan and execute a task graph for `goal`.
pub async fn execute(
    ctx: &CliContext,
    goal: &str,
    port: Option<u16>,
    model: Option<String>,
    ctx_size: Option<String>,
    max_replans: u32,
    hitl: Option<&str>,
) -> Result<()> {
    let hitl_mode = parse_hitl_mode(hitl)?;

    let (ports, handle) = init_session(ctx, port, model, ctx_size).await?;

    let config = CouncilConfig {
        max_replans,
        hitl_mode,
        approval_registry: Some(
            Arc::clone(&ctx.approval_registry) as Arc<dyn CouncilApprovalRegistryPort>
        ),
        repository: Some(Arc::clone(&ctx.council_repo) as Arc<dyn CouncilRepositoryPort>),
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
