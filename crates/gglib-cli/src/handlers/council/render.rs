//! Terminal rendering for [`CouncilEvent`] variants.
//!
//! Exported to sibling modules (`run`, `resume`) via `pub(crate)`.
//! Approval prompts live in [`super::approve`].

use std::sync::Arc;

use gglib_app_services::CouncilApprovalRegistry;
use gglib_core::domain::council::events::CouncilEvent;
use gglib_core::domain::council::task_graph::TaskGraph;

use crate::presentation::{dag, style};

use super::approve::{self, ApproveOpts};

/// Render a single [`CouncilEvent`] to the terminal (or as JSONL when
/// `json_mode` is `true`).
///
/// `last_graph` is updated whenever a `PlanProposed` event arrives so that
/// `AwaitingApproval` handlers can offer the `[e]dit` option.
///
/// In `json_mode` the event is serialised as a JSON line to **stdout** and
/// the function returns immediately — no ASCII art, colors, or interactive
/// prompts are emitted.  All other diagnostic output already goes to
/// **stderr**, so stdout remains clean JSONL.
pub(crate) async fn render_event(
    event: &CouncilEvent,
    approval_registry: &Arc<CouncilApprovalRegistry>,
    last_graph: &mut Option<TaskGraph>,
    opts: &ApproveOpts,
    json_mode: bool,
) {
    if json_mode {
        match serde_json::to_string(event) {
            Ok(line) => println!("{line}"),
            Err(e) => eprintln!("warn: failed to serialise event: {e}"),
        }
        return;
    }
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
            approve::prompt_and_resolve(approval_id, kind, approval_registry, last_graph.as_ref(), opts).await;
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
        CouncilEvent::NodeTextDelta { delta, .. } => {
            eprint!("{delta}");
        }
        CouncilEvent::NodeReasoningDelta { node_id, delta } => {
            eprint!(
                "{}[{node_id}]{}<think>{} {delta}{}",
                dag::node_color(node_id),
                style::RESET,
                style::DIM,
                style::RESET
            );
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
            eprintln!(
                "\n{}[{node_id}]{} {}compacting output…{}",
                dag::node_color(node_id),
                style::RESET,
                style::DIM,
                style::RESET
            );
        }
        CouncilEvent::NodeComplete { node_id, .. } => {
            eprintln!(
                "{}[{node_id}]{} {}✓ complete{}",
                dag::node_color(node_id),
                style::RESET,
                style::SUCCESS,
                style::RESET
            );
        }
        CouncilEvent::NodeFailed { node_id, error } => {
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
            eprintln!(
                "{}[wave {applied_at_wave}] ↩ steering applied: {:?}{}",
                style::DIM,
                diff,
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
