//! CLI handler for `gglib chat council` — council suggestion, editing, and
//! deliberation.
//!
//! Modes: `--suggest` (JSON), `--config` (scripted run), `--config --edit`
//! (edit then run), or bare topic (interactive: suggest → edit → run).
//!
//! When `--port` is omitted the handler auto-starts a llama-server for the
//! default model (or the model given via `--model`), matching the behaviour
//! of `gglib question`.

mod editor;
mod render;
mod repl;
mod stream;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context as _, Result, anyhow};
use tokio::sync::mpsc;

use gglib_agent::council::config::CouncilAgent;
use gglib_agent::council::config::CouncilConfig;
use gglib_agent::council::config::SuggestedCouncil;
use gglib_agent::council::events::{COUNCIL_EVENT_CHANNEL_CAPACITY, CouncilEvent};
use gglib_agent::council::{run_council, suggest_council};
use gglib_core::domain::agent::AgentConfig;
use gglib_core::{AgentMessage, AssistantContent, ProcessHandle, ServerConfig};
use gglib_runtime::CouncilPorts;
use gglib_runtime::compose_council_ports;
use gglib_runtime::llama::args::{resolve_jinja_flag, resolve_reasoning_format};

use crate::bootstrap::CliContext;
use crate::presentation::style;

// ─── Suggest ────────────────────────────────────────────────────────────────

/// Run `--suggest` mode: ask the LLM to design a council and print the
/// resulting `SuggestedCouncil` as pretty-printed JSON.
pub async fn execute_suggest(
    ctx: &CliContext,
    topic: &str,
    port: Option<u16>,
    agent_count: u32,
    model: Option<String>,
) -> Result<()> {
    let (ports, handle) = init_session(ctx, port, model).await?;
    let res = suggest_council(ports.llm, ports.tool_executor, topic, agent_count, None).await;
    stop_server(ctx, &handle).await;
    let council = res?;
    println!("{}", serde_json::to_string_pretty(&council)?);
    Ok(())
}

// ─── Run ────────────────────────────────────────────────────────────────────

/// Run a council deliberation from a saved config file.
pub async fn execute_run(
    ctx: &CliContext,
    config_path: &PathBuf,
    topic: &str,
    port: Option<u16>,
    model: Option<String>,
) -> Result<()> {
    let config = load_config(config_path, topic)?;
    let (ports, handle) = init_session(ctx, port, model).await?;
    let res = run_with_ports(config, ports).await;
    stop_server(ctx, &handle).await;
    res
}

// ─── Interactive (suggest → edit → run) ─────────────────────────────────────

/// Interactive mode: suggest a council, open the REPL editor, then run.
pub async fn execute_interactive(
    ctx: &CliContext,
    topic: &str,
    port: Option<u16>,
    agent_count: u32,
    model: Option<String>,
) -> Result<()> {
    let (ports, handle) = init_session(ctx, port, model).await?;
    let suggested = suggest_council(
        Arc::clone(&ports.llm),
        Arc::clone(&ports.tool_executor),
        topic,
        agent_count,
        None,
    )
    .await?;

    render::render_suggested(&suggested);
    let mut config = suggested.into_config(topic.to_owned());
    let res = edit_then_run(&mut config, ports).await;
    stop_server(ctx, &handle).await;
    res
}

// ─── Edit (load → edit → run) ───────────────────────────────────────────────

/// Load a saved config, open the REPL editor for tweaks, then run.
pub async fn execute_edit(
    ctx: &CliContext,
    config_path: &PathBuf,
    topic: &str,
    port: Option<u16>,
    model: Option<String>,
) -> Result<()> {
    let mut config = load_config(config_path, topic)?;
    let (ports, handle) = init_session(ctx, port, model).await?;
    render::render_config(&config);
    let res = edit_then_run(&mut config, ports).await;
    stop_server(ctx, &handle).await;
    res
}

// ─── Helpers ────────────────────────────────────────────────────────────────

/// Resolve the server port (auto-start when `--port` is omitted) and compose
/// the council ports.  Returns the ports and an optional handle that **must**
/// be stopped when the session ends.
async fn init_session(
    ctx: &CliContext,
    port: Option<u16>,
    model: Option<String>,
) -> Result<(CouncilPorts, Option<ProcessHandle>)> {
    let (resolved_port, handle) = resolve_port(ctx, port, &model).await?;

    if let Err(e) = ctx.mcp.initialize().await {
        tracing::warn!("MCP initialisation failed: {e}");
    }

    let cwd = std::env::current_dir().ok();

    let ports = compose_council_ports(
        format!("http://127.0.0.1:{resolved_port}"),
        ctx.http_client.clone(),
        model,
        Arc::clone(&ctx.mcp),
        cwd,
    );
    Ok((ports, handle))
}

/// Return `(port, maybe_handle)`.  When a port is supplied the server is
/// treated as externally managed.  Otherwise a llama-server is spawned for
/// the specified (or default) model.
async fn resolve_port(
    ctx: &CliContext,
    port: Option<u16>,
    model_arg: &Option<String>,
) -> Result<(u16, Option<ProcessHandle>)> {
    if let Some(p) = port {
        return Ok((p, None));
    }

    // Resolve the model — explicit arg or default from settings.
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

    style::print_info_banner("Council", "\u{1f3db}\u{fe0f}");
    eprintln!("  Starting llama-server for '{}' \u{2026}", model_id.name);
    style::print_banner_close();

    let h = ctx
        .runner
        .start(server_config)
        .await
        .context("failed to start llama-server")?;
    Ok((h.port, Some(h)))
}

/// Stop the auto-started llama-server, if any.
async fn stop_server(ctx: &CliContext, handle: &Option<ProcessHandle>) {
    if let Some(h) = handle
        && let Err(e) = ctx.runner.stop(h).await
    {
        tracing::warn!("failed to stop llama-server: {e}");
    }
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

    loop {
        match repl::edit_loop(config, &tools)? {
            repl::EditOutcome::Run => return run_with_ports(config.clone(), ports).await,
            repl::EditOutcome::Quit => return Ok(()),
            repl::EditOutcome::Refine(instruction) => {
                eprintln!("{}  Refining council …{}", style::DIM, style::RESET);

                let prev = SuggestedCouncil {
                    agents: config.agents.clone(),
                    rounds: config.rounds,
                    synthesis_guidance: config.synthesis_guidance.clone(),
                    judge: config.judge.clone(),
                };
                let prev_json = serde_json::to_string(&prev)?;

                let history = vec![
                    AgentMessage::User {
                        content: config.topic.clone(),
                    },
                    AgentMessage::Assistant {
                        content: AssistantContent {
                            text: Some(prev_json),
                            tool_calls: vec![],
                        },
                    },
                    AgentMessage::User {
                        content: instruction,
                    },
                ];

                let suggested = suggest_council(
                    Arc::clone(&ports.llm),
                    Arc::clone(&ports.tool_executor),
                    &config.topic,
                    config.agents.len() as u32,
                    Some(history),
                )
                .await?;

                render::render_suggested(&suggested);
                *config = suggested.into_config(config.topic.clone());
            }
            repl::EditOutcome::Fill(idx) => {
                handle_ai_fill(config, idx, &ports).await?;
            }
        }
    }
}

async fn run_with_ports(council: CouncilConfig, ports: CouncilPorts) -> Result<()> {
    let agent_config = AgentConfig::default();
    let (tx, mut rx) = mpsc::channel::<CouncilEvent>(COUNCIL_EVENT_CHANNEL_CAPACITY);

    let cwd = std::env::current_dir().ok();

    tokio::spawn(async move {
        run_council(
            council,
            agent_config,
            ports.llm,
            ports.tool_executor,
            tx,
            cwd,
        )
        .await;
    });

    stream::render_council_stream(&mut rx).await;
    Ok(())
}

/// Call `suggest_council` with a targeted refinement prompt for one agent,
/// show a diff, and apply on confirmation.  Strict plucking: only the
/// matching agent's persona/perspective/contentiousness are extracted.
async fn handle_ai_fill(
    config: &mut CouncilConfig,
    idx: usize,
    ports: &CouncilPorts,
) -> Result<()> {
    let target = &config.agents[idx];
    let name = target.name.clone();
    let old = target.clone();

    eprintln!(
        "{}  Filling details for '{}' \u{2026}{}",
        style::DIM,
        name,
        style::RESET
    );

    // Send only agent names as context — avoids echoing full personas back.
    let roster: Vec<&str> = config.agents.iter().map(|a| a.name.as_str()).collect();
    let refinement = format!(
        "The council already has these agents: [{}]. \
         Generate details for the agent named '{name}' to complement them. \
         Return a JSON with ONLY this one agent in the \"agents\" array \u{2014} do NOT \
         regenerate the other agents. Include id, name, persona (2-3 sentences), \
         perspective (1 sentence), contentiousness (0.0-1.0), rounds, and synthesis_guidance.",
        roster.join(", ")
    );
    let history = vec![
        AgentMessage::User {
            content: config.topic.clone(),
        },
        AgentMessage::User {
            content: refinement,
        },
    ];

    let suggested = suggest_council(
        Arc::clone(&ports.llm),
        Arc::clone(&ports.tool_executor),
        &config.topic,
        1,
        Some(history),
    )
    .await?;

    // Strict plucking: find matching agent by name, fallback to same index.
    let filled = pluck_agent(&suggested.agents, &name, idx);
    let Some(filled) = filled else {
        eprintln!(
            "  \x1b[31mThe LLM did not return an agent named '{}' — no changes applied.{}",
            name,
            style::RESET
        );
        return Ok(());
    };

    // Build a preview agent with only the three target fields changed.
    let preview = CouncilAgent {
        persona: filled.persona.clone(),
        perspective: filled.perspective.clone(),
        contentiousness: filled.contentiousness,
        ..old.clone()
    };

    render::render_agent_diff(idx, &old, &preview);

    eprint!("  Apply these changes? [Y/n] ");
    let _ = std::io::Write::flush(&mut std::io::stderr());
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer)?;
    if matches!(answer.trim(), "" | "y" | "Y" | "yes") {
        config.agents[idx].persona = filled.persona.clone();
        config.agents[idx].perspective = filled.perspective.clone();
        config.agents[idx].contentiousness = filled.contentiousness;
        render::render_config(config);
    } else {
        eprintln!("  {}Discarded.{}", style::DIM, style::RESET);
    }
    Ok(())
}

/// Find an agent by name in the suggestion, falling back to same index.
fn pluck_agent<'a>(agents: &'a [CouncilAgent], name: &str, idx: usize) -> Option<&'a CouncilAgent> {
    agents
        .iter()
        .find(|a| a.name == name)
        .or_else(|| agents.get(idx))
}
