//! CLI handler for `gglib council` — council suggestion and deliberation.
//!
//! Two modes:
//! - `--suggest "<topic>"` — ask the LLM to design a council, print JSON
//! - `--config council.json "<topic>"` — run a council deliberation, stream
//!   events to stdout with ANSI-coloured agent labels

use std::io::{self, Write as _};
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use tokio::sync::mpsc;

use gglib_agent::AgentLoop;
use gglib_agent::council::config::{CouncilConfig, SuggestedCouncil};
use gglib_agent::council::events::{COUNCIL_EVENT_CHANNEL_CAPACITY, CouncilEvent};
use gglib_agent::council::prompts::COUNCIL_DESIGNER_PROMPT;
use gglib_agent::council::run_council;
use gglib_core::AGENT_EVENT_CHANNEL_CAPACITY;
use gglib_core::domain::agent::{AgentConfig, AgentEvent, AgentMessage};
use gglib_runtime::compose_council_ports;

use crate::bootstrap::CliContext;
use crate::presentation::style::{BOLD, DIM, RESET};

// ─── Temperature colours (contentiousness → ANSI 256-colour) ────────────────

/// Map contentiousness to an ANSI 256-colour code for terminal display.
/// Mirrors the 5-tier mapping from `prompts::contentiousness_to_instruction`.
fn temperature_fg(c: f32) -> &'static str {
    if c < 0.2 {
        "\x1b[38;5;37m" // teal — collaborative
    } else if c < 0.4 {
        "\x1b[38;5;35m" // emerald — constructive
    } else if c < 0.6 {
        "\x1b[38;5;249m" // neutral grey — balanced
    } else if c < 0.8 {
        "\x1b[38;5;214m" // amber — adversarial
    } else {
        "\x1b[38;5;196m" // red — devil's advocate
    }
}

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

    #[allow(clippy::literal_string_with_formatting_args)]
    let system = COUNCIL_DESIGNER_PROMPT
        .replace("{agent_count}", &agent_count.to_string())
        .replace("{user_topic}", topic);

    let messages = vec![
        AgentMessage::System { content: system },
        AgentMessage::User {
            content: topic.to_owned(),
        },
    ];

    let mut config = AgentConfig::default();
    config.max_iterations = 1;

    let agent = AgentLoop::build(ports.llm, ports.tool_executor, None);
    let (tx, mut rx) = mpsc::channel::<AgentEvent>(AGENT_EVENT_CHANNEL_CAPACITY);

    let handle = tokio::spawn(async move { agent.run(messages, config, tx).await });

    let mut content = String::new();
    while let Some(event) = rx.recv().await {
        if let AgentEvent::FinalAnswer { content: answer } = event {
            content = answer;
        }
    }
    let _ = handle.await;

    if content.is_empty() {
        return Err(anyhow!("LLM did not return a council suggestion"));
    }

    let council: SuggestedCouncil = parse_suggested_council(&content)?;
    let json = serde_json::to_string_pretty(&council)?;
    println!("{json}");
    Ok(())
}

/// Strip markdown fences and parse JSON.
fn parse_suggested_council(raw: &str) -> Result<SuggestedCouncil> {
    let trimmed = strip_markdown_json(raw);
    serde_json::from_str(trimmed)
        .map_err(|e| anyhow!("failed to parse council suggestion: {e}\n\nRaw:\n{raw}"))
}

fn strip_markdown_json(s: &str) -> &str {
    let s = s.trim();
    let s = s.strip_prefix("```json").unwrap_or(s);
    let s = s.strip_prefix("```").unwrap_or(s);
    let s = s.strip_suffix("```").unwrap_or(s);
    s.trim()
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

    // Override topic if the config has a placeholder
    if council.topic.is_empty() {
        council.topic = topic.to_owned();
    }

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

    render_council_stream(&mut council_rx).await;
    Ok(())
}

// ─── Terminal renderer ──────────────────────────────────────────────────────

async fn render_council_stream(rx: &mut mpsc::Receiver<CouncilEvent>) {
    let mut current_agent_color = "";
    let mut in_synthesis = false;

    while let Some(event) = rx.recv().await {
        match event {
            CouncilEvent::AgentTurnStart {
                agent_name,
                round,
                contentiousness,
                ..
            } => {
                let color = temperature_fg(contentiousness);
                current_agent_color = color;
                in_synthesis = false;
                eprintln!("\n{color}{BOLD}── {agent_name}{RESET}  {DIM}(round {round}){RESET}");
            }

            CouncilEvent::AgentTextDelta { delta, .. } => {
                print!("{delta}");
                let _ = io::stdout().flush();
            }

            CouncilEvent::AgentReasoningDelta { delta, .. } => {
                eprint!("{DIM}{delta}{RESET}");
                let _ = io::stderr().flush();
            }

            CouncilEvent::AgentToolCallStart {
                display_name,
                args_summary,
                ..
            } => match args_summary {
                Some(summary) => {
                    eprintln!(
                        "  {DIM}⚙{RESET}  {BOLD}{display_name}{RESET}  {DIM}{summary}{RESET} …"
                    );
                }
                None => {
                    eprintln!("  {DIM}⚙{RESET}  {BOLD}{display_name}{RESET} …");
                }
            },

            CouncilEvent::AgentToolCallComplete {
                display_name,
                duration_display,
                result,
                ..
            } => {
                let icon = if result.success { "✓" } else { "✗" };
                let icon_color = if result.success {
                    "\x1b[32m"
                } else {
                    "\x1b[31m"
                };
                eprintln!(
                    "  {icon_color}{icon}{RESET}  {BOLD}{display_name}{RESET}  {DIM}{duration_display}{RESET}"
                );
            }

            CouncilEvent::AgentTurnComplete { core_claim, .. } => {
                println!();
                if let Some(claim) = core_claim {
                    eprintln!("  {current_agent_color}{DIM}CORE CLAIM: {claim}{RESET}");
                }
            }

            CouncilEvent::RoundSeparator { round } => {
                eprintln!("\n{DIM}═══════════════════ Round {round} ═══════════════════{RESET}");
            }

            CouncilEvent::SynthesisStart => {
                in_synthesis = true;
                eprintln!("\n\x1b[36m{BOLD}── Council Synthesis ──{RESET}");
            }

            CouncilEvent::SynthesisTextDelta { delta } => {
                print!("{delta}");
                let _ = io::stdout().flush();
            }

            CouncilEvent::SynthesisComplete { .. } => {
                println!();
            }

            CouncilEvent::CouncilError { message } => {
                eprintln!("\n  \x1b[31m❌  {message}{RESET}");
            }

            CouncilEvent::CouncilComplete => {
                if !in_synthesis {
                    println!();
                }
                eprintln!("{DIM}Council complete.{RESET}");
            }
        }
    }
}
