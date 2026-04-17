//! CLI handler for `gglib council` — council suggestion, editing, and
//! deliberation.
//!
//! Modes: `--suggest` (JSON), `--config` (scripted run), `--config --edit`
//! (edit then run), or bare topic (interactive: suggest → edit → run).

mod editor;
mod render;
mod repl;
mod stream;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use tokio::sync::mpsc;

use gglib_agent::council::config::CouncilConfig;
use gglib_agent::council::events::{COUNCIL_EVENT_CHANNEL_CAPACITY, CouncilEvent};
use gglib_agent::council::{run_council, suggest_council};
use gglib_core::domain::agent::AgentConfig;
use gglib_runtime::{CouncilPorts, compose_council_ports};

use crate::bootstrap::CliContext;

// ─── Suggest ────────────────────────────────────────────────────────────────

pub async fn execute_suggest(
    ctx: &CliContext,
    topic: &str,
    port: u16,
    agent_count: u32,
    model: Option<String>,
) -> Result<()> {
    let ports = init_ports(ctx, port, model).await;
    let council = suggest_council(ports.llm, ports.tool_executor, topic, agent_count).await?;
    println!("{}", serde_json::to_string_pretty(&council)?);
    Ok(())
}

// ─── Run ────────────────────────────────────────────────────────────────────

pub async fn execute_run(
    ctx: &CliContext,
    config_path: &PathBuf,
    topic: &str,
    port: u16,
    model: Option<String>,
) -> Result<()> {
    let config = load_config(config_path, topic)?;
    let ports = init_ports(ctx, port, model).await;
    run_with_ports(config, ports).await
}

// ─── Interactive (suggest → edit → run) ─────────────────────────────────────

pub async fn execute_interactive(
    ctx: &CliContext,
    topic: &str,
    port: u16,
    agent_count: u32,
    model: Option<String>,
) -> Result<()> {
    let ports = init_ports(ctx, port, model).await;
    let suggested = suggest_council(
        Arc::clone(&ports.llm),
        Arc::clone(&ports.tool_executor),
        topic,
        agent_count,
    )
    .await?;

    render::render_suggested(&suggested);
    let mut config = suggested.into_config(topic.to_owned());
    edit_then_run(&mut config, ports).await
}

// ─── Edit (load → edit → run) ───────────────────────────────────────────────

pub async fn execute_edit(
    ctx: &CliContext,
    config_path: &PathBuf,
    topic: &str,
    port: u16,
    model: Option<String>,
) -> Result<()> {
    let mut config = load_config(config_path, topic)?;
    let ports = init_ports(ctx, port, model).await;
    render::render_config(&config);
    edit_then_run(&mut config, ports).await
}

// ─── Helpers ────────────────────────────────────────────────────────────────

async fn init_ports(ctx: &CliContext, port: u16, model: Option<String>) -> CouncilPorts {
    if let Err(e) = ctx.mcp.initialize().await {
        tracing::warn!("MCP initialisation failed: {e}");
    }
    compose_council_ports(
        format!("http://127.0.0.1:{port}"),
        ctx.http_client.clone(),
        model,
        Arc::clone(&ctx.mcp),
    )
}

fn load_config(path: &PathBuf, topic: &str) -> Result<CouncilConfig> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| anyhow!("cannot read config file '{}': {e}", path.display()))?;
    let mut config: CouncilConfig =
        serde_json::from_str(&raw).map_err(|e| anyhow!("invalid council config: {e}"))?;
    if config.topic.is_empty() {
        config.topic = topic.to_owned();
    }
    Ok(config)
}

async fn edit_then_run(config: &mut CouncilConfig, ports: CouncilPorts) -> Result<()> {
    let tools: Vec<String> = ports
        .tool_executor
        .list_tools()
        .await
        .into_iter()
        .map(|t| t.name)
        .collect();
    match repl::edit_loop(config, &tools)? {
        Some(()) => run_with_ports(config.clone(), ports).await,
        None => Ok(()),
    }
}

async fn run_with_ports(council: CouncilConfig, ports: CouncilPorts) -> Result<()> {
    let agent_config = AgentConfig::default();
    let (tx, mut rx) = mpsc::channel::<CouncilEvent>(COUNCIL_EVENT_CHANNEL_CAPACITY);

    tokio::spawn(async move {
        run_council(council, agent_config, ports.llm, ports.tool_executor, tx).await;
    });

    stream::render_council_stream(&mut rx).await;
    Ok(())
}
