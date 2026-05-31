//! Terminal rendering for [`CouncilEvent`] and HITL approval prompts.
//!
//! Exported to sibling modules (`run`, `resume`) via `pub(crate)`.

use std::sync::Arc;

use gglib_app_services::CouncilApprovalRegistry;
use gglib_core::domain::council::events::{ApprovalKind, CouncilEvent};
use gglib_core::ports::{ApprovalDecision, CouncilApprovalRegistryPort as _};

use crate::presentation::{dag, style};

/// Render a single [`CouncilEvent`] to the terminal.
///
/// For `AwaitingApproval` events, prompts the user interactively and
/// resolves the approval via the registry.
pub(crate) async fn render_event(
    event: &CouncilEvent,
    approval_registry: &Arc<CouncilApprovalRegistry>,
) {
    match event {
        CouncilEvent::PlanProposed { graph } => {
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
            prompt_and_resolve(approval_id, kind, approval_registry).await;
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

/// Prompt the user for an approval decision and resolve it in the registry.
pub(crate) async fn prompt_and_resolve(
    approval_id: &str,
    kind: &ApprovalKind,
    registry: &Arc<CouncilApprovalRegistry>,
) {
    let description = match kind {
        ApprovalKind::Plan => "the proposed plan".to_owned(),
        ApprovalKind::Node { node_id } => format!("node '{node_id}'"),
        ApprovalKind::Tool { node_id, tool_name } => {
            format!("tool call '{tool_name}' in node '{node_id}'")
        }
        ApprovalKind::SpawnSubteam { node_id, .. } => {
            format!("spawn subteam requested by node '{node_id}'")
        }
    };

    eprintln!(
        "\n{}  ⏸  Awaiting approval for {description}{}",
        style::WARNING,
        style::RESET
    );
    eprintln!("  [y] approve  [n] reject  (Enter = approve)");
    eprint!("  Decision: ");

    let input = tokio::task::spawn_blocking(|| {
        let mut buf = String::new();
        let _ = std::io::stdin().read_line(&mut buf);
        buf.trim().to_lowercase()
    })
    .await
    .unwrap_or_default();

    let decision = match input.as_str() {
        "n" | "no" | "reject" => {
            eprint!("  Rejection reason (optional): ");
            let reason = tokio::task::spawn_blocking(|| {
                let mut buf = String::new();
                let _ = std::io::stdin().read_line(&mut buf);
                buf.trim().to_owned()
            })
            .await
            .unwrap_or_default();
            let reason = if reason.is_empty() { None } else { Some(reason) };
            ApprovalDecision::Reject(reason.unwrap_or_else(|| "rejected by user".to_owned()))
        }
        _ => ApprovalDecision::Approve,
    };

    registry.resolve(approval_id, decision);
}
