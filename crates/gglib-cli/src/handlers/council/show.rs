//! `gglib council show <run-id>` — inspect a specific orchestrator run.
//!
//! Prints run metadata, the saved task-graph tree, and a milestone event
//! timeline (plan proposed, nodes started/completed/failed, final status).

use anyhow::{Result, anyhow};

use gglib_core::domain::council::events::CouncilEvent;
use gglib_core::domain::council::run::CouncilRunStatus;
use gglib_core::ports::CouncilRepositoryPort as _;

use crate::bootstrap::CliContext;
use crate::presentation::{dag, style, tables};

/// Print full details for run `run_id`.
pub async fn execute(ctx: &CliContext, run_id: &str) -> Result<()> {
    let run = ctx
        .council_repo
        .get_run(run_id)
        .await
        .map_err(|e| anyhow!("failed to load run: {e}"))?
        .ok_or_else(|| anyhow!("run '{run_id}' not found"))?;

    let created = tables::format_relative_time(&run.created_at);
    let updated = tables::format_relative_time(&run.updated_at);

    style::print_info_banner("Run", "\u{1f50d}");
    eprintln!("  {}ID:{}      {}", style::BOLD, style::RESET, run.id);
    eprintln!("  {}Goal:{}    {}", style::BOLD, style::RESET, run.goal);
    eprintln!(
        "  {}Status:{}  {}{}{}",
        style::BOLD,
        style::RESET,
        status_color(&run.status),
        run.status,
        style::RESET
    );
    eprintln!(
        "  {}Created:{} {} (updated {})",
        style::BOLD,
        style::RESET,
        created,
        updated
    );
    style::print_banner_close();

    // Task graph tree (if a graph has been saved).
    if let Some(graph_result) = run.graph() {
        match graph_result {
            Ok(graph) => {
                eprintln!();
                dag::render_tree(&graph, &mut std::io::stderr());
            }
            Err(e) => {
                eprintln!(
                    "{}  (could not parse saved graph: {e}){}",
                    style::DIM,
                    style::RESET
                );
            }
        }
    }

    // Milestone event timeline.
    let events = ctx
        .council_repo
        .list_events(run_id)
        .await
        .map_err(|e| anyhow!("failed to load events: {e}"))?;

    if !events.is_empty() {
        eprintln!("\n{}─── Timeline ───{}", style::BOLD, style::RESET);
        for record in &events {
            let Ok(event) = serde_json::from_str::<CouncilEvent>(&record.event_json) else {
                continue;
            };
            if let Some(line) = milestone_line(&event) {
                eprintln!("  [wave {}] {}", record.wave_index, line);
            }
        }
    }

    Ok(())
}

fn status_color(status: &CouncilRunStatus) -> &'static str {
    match status {
        CouncilRunStatus::Running => style::INFO,
        CouncilRunStatus::AwaitingApproval => style::WARNING,
        CouncilRunStatus::Completed => style::SUCCESS,
        CouncilRunStatus::Failed => style::DANGER,
        CouncilRunStatus::Interrupted => style::DIM,
    }
}

/// Returns a one-line summary for milestone events; `None` for noise.
fn milestone_line(event: &CouncilEvent) -> Option<String> {
    match event {
        CouncilEvent::PlanProposed { graph } => {
            Some(format!("plan proposed — {} node(s)", graph.nodes.len()))
        }
        CouncilEvent::ReplanAttempt { attempt, reason } => {
            Some(format!("replan attempt {attempt}: {reason}"))
        }
        CouncilEvent::PlanApproved => Some("plan approved".to_owned()),
        CouncilEvent::PlanRejected { reason } => Some(format!(
            "plan rejected{}",
            reason
                .as_deref()
                .map(|r| format!(": {r}"))
                .unwrap_or_default()
        )),
        CouncilEvent::NodeStarted { node_id, .. } => Some(format!(
            "{}[{node_id}]{} started",
            dag::node_color(node_id),
            style::RESET
        )),
        CouncilEvent::NodeComplete { node_id, .. } => Some(format!(
            "{}[{node_id}]{} {}complete{}",
            dag::node_color(node_id),
            style::RESET,
            style::SUCCESS,
            style::RESET
        )),
        CouncilEvent::NodeFailed { node_id, error } => Some(format!(
            "{}[{node_id}]{} {}FAILED:{} {error}",
            dag::node_color(node_id),
            style::RESET,
            style::DANGER,
            style::RESET
        )),
        CouncilEvent::CouncilComplete { .. } => Some(format!(
            "{}run complete{}",
            style::SUCCESS,
            style::RESET
        )),
        CouncilEvent::CouncilError { message } => {
            Some(format!("{}error:{} {message}", style::DANGER, style::RESET))
        }
        _ => None,
    }
}
