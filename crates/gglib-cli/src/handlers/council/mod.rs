//! CLI handler for `gglib council` — council suggestion, editing, and
//! deliberation.
//!
//! # Modes
//!
//! - `gglib council "<topic>"` — interactive: suggest → edit → run
//! - `--suggest` — pipe-friendly: print suggested config as JSON
//! - `--config council.json` — scripted: load config and run directly
//! - `--config council.json --edit` — load config, edit interactively, then run
//!
//! # Module layout
//!
//! | File        | Responsibility                                      |
//! |-------------|-----------------------------------------------------|
//! | `mod.rs`    | Command routing and orchestration                   |
//! | `stream.rs` | Live ANSI renderer for `CouncilEvent` SSE stream    |
//! | `render.rs` | Static summary table and agent card display         |
//! | `repl.rs`   | Rustyline editing REPL + command dispatch            |
//! | `editor.rs` | Agent/round state mutation logic                    |

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
use gglib_runtime::compose_council_ports;

use crate::bootstrap::CliContext;

// ─── Suggest ────────────────────────────────────────────────────────────────

pub async fn execute_suggest(
    ctx: &CliContext,
    topic: &str,
    port: u16,
    agent_count: u32,
    model: Option<String>,
) -> Result<()> {
    if let Err(e) = ctx.mcp.initialize().await {
        tracing::warn!("MCP initialisation failed: {e}");
    }

    let ports = compose_council_ports(
        format!("http://127.0.0.1:{port}"),
        ctx.http_client.clone(),
        model,
        Arc::clone(&ctx.mcp),
    );

    let council = suggest_council(ports.llm, ports.tool_executor, topic, agent_count).await?;
    let json = serde_json::to_string_pretty(&council)?;
    println!("{json}");
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
    let raw = std::fs::read_to_string(config_path)
        .map_err(|e| anyhow!("cannot read config file '{}': {e}", config_path.display()))?;
    let mut council: CouncilConfig =
        serde_json::from_str(&raw).map_err(|e| anyhow!("invalid council config: {e}"))?;

    if council.topic.is_empty() {
        council.topic = topic.to_owned();
    }

    run_council_config(ctx, council, port, model).await
}

// ─── Shared run helper ──────────────────────────────────────────────────────

/// Spawn the council orchestrator and stream events to the terminal.
async fn run_council_config(
    ctx: &CliContext,
    council: CouncilConfig,
    port: u16,
    model: Option<String>,
) -> Result<()> {
    if let Err(e) = ctx.mcp.initialize().await {
        tracing::warn!("MCP initialisation failed: {e}");
    }

    let ports = compose_council_ports(
        format!("http://127.0.0.1:{port}"),
        ctx.http_client.clone(),
        model,
        Arc::clone(&ctx.mcp),
    );

    let agent_config = AgentConfig::default();
    let (council_tx, mut council_rx) =
        mpsc::channel::<CouncilEvent>(COUNCIL_EVENT_CHANNEL_CAPACITY);

    tokio::spawn(async move {
        run_council(
            council,
            agent_config,
            ports.llm,
            ports.tool_executor,
            council_tx,
        )
        .await;
    });

    stream::render_council_stream(&mut council_rx).await;
    Ok(())
}
