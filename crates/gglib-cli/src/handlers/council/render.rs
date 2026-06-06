//! Terminal rendering for [`CouncilEvent`] variants.
//!
//! Exported to sibling modules (`run`, `resume`) via `pub(crate)`.
//! Approval prompts live in [`super::approve`].

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use gglib_app_services::CouncilApprovalRegistry;
use gglib_core::domain::council::events::CouncilEvent;
use gglib_core::domain::council::task_graph::TaskGraph;
use tokio::sync::mpsc;

use crate::presentation::{dag, style};

use super::approve::{self, ApproveOpts};

// =============================================================================
// Per-node line buffer
// =============================================================================

/// Append `delta` to the per-node line buffer and flush every complete line
/// (terminated by `\n`) to stderr, prefixed with `[node_id] `.
///
/// This prevents interleaved output when multiple worker nodes stream tokens
/// concurrently: characters from different nodes are kept separate until a
/// full line is available, then each line is written atomically.
///
/// `dim` controls whether the flushed lines are rendered in dim style
/// (used for reasoning/thinking deltas).
fn buffer_delta(
    line_buf: &mut HashMap<String, String>,
    node_id: &str,
    node_color: &str,
    delta: &str,
    dim: bool,
) {
    let buf = line_buf.entry(node_id.to_owned()).or_default();
    buf.push_str(delta);
    // Flush every complete newline-terminated segment.
    while let Some(newline_pos) = buf.find('\n') {
        let line: String = buf.drain(..=newline_pos).collect();
        let trimmed = line.trim_end_matches('\n');
        if !trimmed.is_empty() {
            if dim {
                eprintln!(
                    "{node_color}[{node_id}]{} {}{trimmed}{}",
                    style::RESET,
                    style::DIM,
                    style::RESET,
                );
            } else {
                eprintln!("{node_color}[{node_id}]{} {trimmed}", style::RESET);
            }
        } else {
            // Blank line — preserve vertical rhythm.
            eprintln!();
        }
    }
}

/// Flush any buffered partial line for `node_id`, even if it has no trailing
/// newline. Called when a node completes, fails, or is compacted.
fn flush_node_buf(line_buf: &mut HashMap<String, String>, node_id: &str, node_color: &str) {
    if let Some(remaining) = line_buf.remove(node_id) {
        let trimmed = remaining.trim_end();
        if !trimmed.is_empty() {
            eprintln!("{node_color}[{node_id}]{} {trimmed}", style::RESET);
        }
    }
}

/// Mutable rendering state threaded through every [`render_event`] call
/// for a single council run.
///
/// Grouping these into a struct keeps [`render_event`]'s argument count
/// within the clippy `too_many_arguments` limit (≤ 7) while allowing new
/// state to be added without touching every call site.
pub(crate) struct RenderState {
    /// The most-recently seen task graph, used by the HITL approval prompt.
    pub last_graph: Option<TaskGraph>,
    /// Nodes currently emitting reasoning/thinking tokens.
    pub thinking_nodes: HashSet<String>,
    /// Per-node line buffers for interleave-free parallel output.
    pub line_buf: HashMap<String, String>,
}

impl RenderState {
    pub(crate) fn new() -> Self {
        Self {
            last_graph: None,
            thinking_nodes: HashSet::new(),
            line_buf: HashMap::new(),
        }
    }
}

/// Render a single [`CouncilEvent`] to the terminal (or as JSONL when
/// `json_mode` is `true`).
///
/// `state` holds all mutable rendering state for the current run (last
/// graph snapshot, thinking-node set, per-node line buffers).  Create once
/// with [`RenderState::new`] and pass `&mut state` on every call.
///
/// In `json_mode` the event is serialised as a JSON line to **stdout** and
/// the function returns immediately — no ASCII art, colors, or interactive
/// prompts are emitted.  All other diagnostic output already goes to
/// **stderr**, so stdout remains clean JSONL.
pub(crate) async fn render_event(
    event: &CouncilEvent,
    approval_registry: &Arc<CouncilApprovalRegistry>,
    state: &mut RenderState,
    opts: &ApproveOpts,
    json_mode: bool,
    input_rx: &mut mpsc::UnboundedReceiver<String>,
) {
    if json_mode {
        match serde_json::to_string(event) {
            Ok(line) => println!("{line}"),
            Err(e) => eprintln!("warn: failed to serialise event: {e}"),
        }
        return;
    }
    let RenderState {
        last_graph,
        thinking_nodes,
        line_buf,
    } = state;
    match event {
        CouncilEvent::PlanProposed { graph } => {
            *last_graph = Some(graph.clone());
            style::print_info_banner("Orchestrate", "\u{1f5fa}\u{fe0f}");
            eprintln!(
                "  {}Plan proposed:{} {} node(s) for goal: {}",
                style::BOLD,
                style::RESET,
                graph.nodes.len(),
                graph.goal
            );
            style::print_banner_close();
            dag::render_tree(graph, &mut std::io::stderr());
            eprintln!();
        }
        CouncilEvent::ReplanAttempt { attempt, reason } => {
            eprintln!(
                "{}  ↻ Replanning (attempt {attempt}): {reason}{}",
                style::DIM,
                style::RESET
            );
        }
        CouncilEvent::PlanApproved => {
            eprintln!(
                "{}  ✓ Plan approved — starting execution{}",
                style::DIM,
                style::RESET
            );
        }
        CouncilEvent::PlanRejected { reason } => {
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
        CouncilEvent::AwaitingApproval { approval_id, kind } => {
            approve::prompt_and_resolve(
                approval_id,
                kind,
                approval_registry,
                last_graph.as_ref(),
                opts,
                input_rx,
            )
            .await;
        }
        CouncilEvent::NodeStarted {
            node_id,
            goal: node_goal,
        } => {
            eprintln!(
                "\n{}[{}]{} {}",
                dag::node_color(node_id),
                node_id,
                style::RESET,
                node_goal
            );
        }
        CouncilEvent::NodeTextDelta { node_id, delta } => {
            if thinking_nodes.remove(node_id.as_str()) {
                eprintln!();
            }
            buffer_delta(line_buf, node_id, dag::node_color(node_id), delta, false);
        }
        CouncilEvent::NodeReasoningDelta { node_id, delta } => {
            if thinking_nodes.insert(node_id.clone()) {
                eprintln!(
                    "{}[{node_id}]{} {}(thinking…){}",
                    dag::node_color(node_id),
                    style::RESET,
                    style::DIM,
                    style::RESET
                );
            }
            buffer_delta(line_buf, node_id, dag::node_color(node_id), delta, true);
        }
        CouncilEvent::NodeToolCallStart {
            node_id,
            display_name,
            args_summary,
            ..
        } => {
            eprintln!(
                "\n{}[{node_id}]{} {}⚙ {}  {}{}",
                dag::node_color(node_id),
                style::RESET,
                style::DIM,
                display_name,
                args_summary.as_deref().unwrap_or(""),
                style::RESET
            );
        }
        CouncilEvent::NodeToolCallComplete {
            node_id,
            display_name,
            duration_display,
            ..
        } => {
            eprintln!(
                "{}[{node_id}]{} {}✓ {}  {}{}",
                dag::node_color(node_id),
                style::RESET,
                style::DIM,
                display_name,
                duration_display,
                style::RESET
            );
        }
        CouncilEvent::NodeSystemWarning {
            node_id, message, ..
        } => {
            eprintln!(
                "{}[{node_id}]{} {}⚠ {}{}",
                dag::node_color(node_id),
                style::RESET,
                style::WARNING,
                message,
                style::RESET
            );
        }
        CouncilEvent::NodeCompacting { node_id } => {
            flush_node_buf(line_buf, node_id, dag::node_color(node_id));
            eprintln!(
                "\n{}[{node_id}]{} {}compacting output…{}",
                dag::node_color(node_id),
                style::RESET,
                style::DIM,
                style::RESET
            );
        }
        CouncilEvent::NodeComplete { node_id, .. } => {
            flush_node_buf(line_buf, node_id, dag::node_color(node_id));
            eprintln!(
                "{}[{node_id}]{} {}✓ complete{}",
                dag::node_color(node_id),
                style::RESET,
                style::SUCCESS,
                style::RESET
            );
        }
        CouncilEvent::NodeFailed { node_id, error } => {
            flush_node_buf(line_buf, node_id, dag::node_color(node_id));
            eprintln!(
                "{}[{node_id}]{} {}✗ failed: {error}{}",
                dag::node_color(node_id),
                style::RESET,
                style::DANGER,
                style::RESET
            );
        }
        CouncilEvent::SynthesisStart => {
            eprintln!("\n{}─── Synthesis ───{}", style::BOLD, style::RESET);
        }
        CouncilEvent::SynthesisTextDelta { delta } => {
            eprint!("{delta}");
        }
        CouncilEvent::SynthesisComplete { .. } => {
            eprintln!();
        }
        CouncilEvent::CouncilComplete { answer } => {
            eprintln!("\n{}─── Final Answer ───{}", style::BOLD, style::RESET);
            println!("{answer}");
        }
        CouncilEvent::CouncilError { message } => {
            eprintln!("{}Error: {message}{}", style::DANGER, style::RESET);
        }
        CouncilEvent::TeamStarted { team_id, .. } => {
            eprintln!("{}[{team_id}] ▶ team started{}", style::DIM, style::RESET);
        }
        CouncilEvent::TeamSynthesized { team_id, .. } => {
            eprintln!(
                "{}[{team_id}] ✓ team synthesized{}",
                style::DIM,
                style::RESET
            );
        }
        CouncilEvent::NodeProgress { .. }
        | CouncilEvent::SynthesisProgress { .. }
        | CouncilEvent::RunCostEstimate { .. }
        | CouncilEvent::SubteamSpawned { .. }
        | CouncilEvent::WaveCompleted { .. } => {}
        CouncilEvent::SteeringApplied {
            applied_at_wave,
            diff,
        } => {
            let diff_str = serde_json::to_string(diff).unwrap_or_else(|_| format!("{diff:?}"));
            eprintln!(
                "{}  ↩  Steering applied at wave {applied_at_wave}:{} {diff_str}",
                style::INFO,
                style::RESET,
            );
        }
        CouncilEvent::DebateRoundStarted { node_id, round } => {
            eprintln!(
                "{}[{node_id}]{} {}◆ debate round {round}{}",
                dag::node_color(node_id),
                style::RESET,
                style::DIM,
                style::RESET
            );
        }
        CouncilEvent::DebateAgentTurnStarted {
            node_id,
            agent_name,
            round,
            ..
        } => {
            eprintln!(
                "\n{}[{node_id}]{} {}[{agent_name}] round {round}{}",
                dag::node_color(node_id),
                style::RESET,
                style::DIM,
                style::RESET
            );
        }
        CouncilEvent::DebateAgentTextDelta { delta, .. } => {
            eprint!("{delta}");
        }
        CouncilEvent::DebateAgentReasoningDelta { node_id, delta, .. } => {
            eprint!(
                "{}[{node_id}]{}<think>{} {delta}{}",
                dag::node_color(node_id),
                style::RESET,
                style::DIM,
                style::RESET
            );
        }
        CouncilEvent::DebateAgentToolCallStart {
            node_id,
            display_name,
            args_summary,
            ..
        } => {
            eprintln!(
                "\n{}[{node_id}]{} {}⚙ {}  {}{}",
                dag::node_color(node_id),
                style::RESET,
                style::DIM,
                display_name,
                args_summary.as_deref().unwrap_or(""),
                style::RESET
            );
        }
        CouncilEvent::DebateAgentToolCallComplete {
            node_id,
            display_name,
            duration_display,
            ..
        } => {
            eprintln!(
                "{}[{node_id}]{} {}✓ {}  {}{}",
                dag::node_color(node_id),
                style::RESET,
                style::DIM,
                display_name,
                duration_display,
                style::RESET
            );
        }
        CouncilEvent::DebateAgentTurnComplete { .. } => {}
        CouncilEvent::DebateJudgeStarted { node_id, round } => {
            eprint!(
                "\n{}[{node_id}]{} {}⚖ judging round {round}…{}",
                dag::node_color(node_id),
                style::RESET,
                style::DIM,
                style::RESET
            );
        }
        CouncilEvent::DebateJudgeTextDelta { delta, .. } => {
            eprint!("{delta}");
        }
        CouncilEvent::DebateJudgeSummary {
            node_id,
            round,
            consensus_reached,
            ..
        } => {
            let verdict = if *consensus_reached {
                "consensus reached"
            } else {
                "continuing"
            };
            eprintln!(
                "\n{}[{node_id}]{} {}⚖ round {round}: {verdict}{}",
                dag::node_color(node_id),
                style::RESET,
                style::DIM,
                style::RESET
            );
        }
        CouncilEvent::DebateRoundCompacted { .. } => {}
        CouncilEvent::DebateStanceMap { node_id, .. } => {
            eprintln!(
                "{}[{node_id}]{} {}stances recorded{}",
                dag::node_color(node_id),
                style::RESET,
                style::DIM,
                style::RESET
            );
        }
        CouncilEvent::DebateSynthesisStarted { node_id } => {
            eprintln!(
                "\n{}[{node_id}]{} {}─── Debate Synthesis ───{}",
                dag::node_color(node_id),
                style::RESET,
                style::BOLD,
                style::RESET
            );
        }
        CouncilEvent::DebateSynthesisTextDelta { delta, .. } => {
            eprint!("{delta}");
        }
        CouncilEvent::DebateSynthesisComplete { .. } => {
            eprintln!();
        }
    }
}
