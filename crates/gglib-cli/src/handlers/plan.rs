//! CLI handler for `gglib plan` — decompose a goal into a task-graph DAG.
//!
//! Calls the director agent, then renders the resulting [`TaskGraph`] as:
//! - An indented tree (stdout) showing each node and its dependencies.
//! - A Mermaid diagram (stdout) ready to paste into documentation.
//!
//! A llama-server is started automatically when `--port` is omitted.

use std::sync::Arc;

use anyhow::{Context as _, Result, anyhow};

use gglib_agent::council::plan;
use gglib_core::domain::council::task_graph::{HitlMode, NodeId, TaskGraph};
use gglib_core::{ProcessHandle, ServerConfig};
use gglib_runtime::CouncilPorts;
use gglib_runtime::compose_council_ports;
use gglib_runtime::llama::args::{
    ContextInput, resolve_context_size, resolve_jinja_flag, resolve_reasoning_format,
};

use crate::bootstrap::CliContext;
use crate::presentation::style;

// ─── Execute ────────────────────────────────────────────────────────────────

/// Run `gglib plan "<goal>"`.
///
/// Resolves the LLM port, calls the director, then prints an indented tree
/// and Mermaid diagram to stdout.
pub async fn execute(
    ctx: &CliContext,
    goal: &str,
    port: Option<u16>,
    model: Option<String>,
    ctx_size: Option<String>,
    max_replans: u32,
) -> Result<()> {
    let (ports, handle) = init_session(ctx, port, model, ctx_size).await?;

    eprintln!("{}  Planning: {}{}…", style::DIM, style::RESET, goal);

    let res = plan(goal, &[], ports.llm, HitlMode::None, max_replans, None).await;

    stop_server(ctx, &handle).await;

    let graph = res.map_err(|e| anyhow!("{e}"))?;

    render_tree(&graph);
    println!();
    render_mermaid(&graph);

    Ok(())
}

// ─── Rendering ──────────────────────────────────────────────────────────────

/// Render the task graph as an indented tree to stdout.
///
/// # Format
///
/// ```text
/// ── goal: Research the history of llama.cpp and write a summary
///    ├── [research] Research the history and development…
///    └── [write-summary] Write a clear, comprehensive… (needs: research)
/// ```
fn render_tree(graph: &TaskGraph) {
    println!("{}── goal:{} {}", style::BOLD, style::RESET, graph.goal);

    // Topological order: roots first, then nodes that depend on completed ones.
    let ordered = topological_order(graph);
    let last = ordered.len().saturating_sub(1);

    for (i, id) in ordered.iter().enumerate() {
        let node = &graph.nodes[id];
        let is_last = i == last;
        let connector = if is_last {
            "   └──"
        } else {
            "   ├──"
        };
        let deps = if node.depends_on.is_empty() {
            String::new()
        } else {
            let dep_ids: Vec<&str> = node.depends_on.iter().map(|d| d.0.as_str()).collect();
            format!(
                " {}(needs: {}){}",
                style::DIM,
                dep_ids.join(", "),
                style::RESET
            )
        };
        println!(
            "{} {}[{}]{} {}{}",
            connector,
            style::INFO,
            id,
            style::RESET,
            node.goal,
            deps,
        );
    }
}

/// Render the task graph as a Mermaid flowchart diagram to stdout.
///
/// # Format
///
/// ```text
/// ```mermaid
/// flowchart LR
///     research["Research the history…"]
///     write-summary["Write a clear…"]
///     research --> write-summary
/// ```
/// ```
fn render_mermaid(graph: &TaskGraph) {
    println!("```mermaid");
    println!("flowchart LR");

    // Node definitions.
    for (id, node) in &graph.nodes {
        // Truncate long goals to 50 chars for readability.
        let label = if node.goal.len() > 50 {
            format!("{}…", &node.goal[..50])
        } else {
            node.goal.clone()
        };
        // Escape double-quotes inside labels.
        let label = label.replace('"', "'");
        println!("    {}[\"{}\"]", id, label);
    }

    // Edges.
    for (id, node) in &graph.nodes {
        for dep in &node.depends_on {
            println!("    {} --> {}", dep, id);
        }
    }

    println!("```");
}

/// Return node ids in a topological (dependency-first) order.
fn topological_order(graph: &TaskGraph) -> Vec<&NodeId> {
    use std::collections::{HashMap, HashSet, VecDeque};

    let mut in_degree: HashMap<&NodeId, usize> = graph.nodes.keys().map(|id| (id, 0)).collect();
    let mut dependents: HashMap<&NodeId, Vec<&NodeId>> = HashMap::new();

    for (id, node) in &graph.nodes {
        for dep in &node.depends_on {
            *in_degree.entry(id).or_insert(0) += 1;
            dependents.entry(dep).or_default().push(id);
        }
    }

    let mut queue: VecDeque<&NodeId> = in_degree
        .iter()
        .filter(|(_, d)| **d == 0)
        .map(|(&id, _)| id)
        .collect();

    // Stable sort within the same depth level.
    let mut queue_vec: Vec<&NodeId> = queue.drain(..).collect();
    queue_vec.sort_by_key(|id| &id.0);
    let mut queue: VecDeque<&NodeId> = queue_vec.into_iter().collect();

    let mut result = Vec::new();
    let mut visited: HashSet<&NodeId> = HashSet::new();

    while let Some(id) = queue.pop_front() {
        if visited.contains(id) {
            continue;
        }
        visited.insert(id);
        result.push(id);

        if let Some(children) = dependents.get(id) {
            let mut children: Vec<&NodeId> = children.clone();
            children.sort_by_key(|id| &id.0);
            for child in children {
                let entry = in_degree.entry(child).or_insert(0);
                if *entry > 0 {
                    *entry -= 1;
                }
                if *entry == 0 && !visited.contains(child) {
                    queue.push_back(child);
                }
            }
        }
    }

    result
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

    style::print_info_banner("Plan", "\u{1f5fa}\u{fe0f}");
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
