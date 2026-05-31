//! `gglib council list [--status]` — list past orchestrator runs.

use anyhow::{Result, anyhow};

use gglib_core::domain::council::run::CouncilRunStatus;
use gglib_core::ports::CouncilRepositoryPort as _;

use crate::bootstrap::CliContext;
use crate::presentation::{style, tables};

/// List past runs, optionally filtered by status.
pub async fn execute(ctx: &CliContext, status: Option<&str>) -> Result<()> {
    let filter = status.map(parse_status).transpose()?;
    let runs = ctx
        .council_repo
        .list_runs(filter)
        .await
        .map_err(|e| anyhow!("failed to list runs: {e}"))?;

    if runs.is_empty() {
        eprintln!("{}No orchestrator runs found.{}", style::DIM, style::RESET);
        return Ok(());
    }

    println!(
        "{}{:<36}  {:<20}  {:<12}  GOAL{}",
        style::BOLD,
        "ID",
        "STATUS",
        "CREATED",
        style::RESET
    );
    tables::print_separator(100);

    for run in &runs {
        let goal = tables::truncate_string(&run.goal, 50);
        let created = tables::format_relative_time(&run.created_at);
        println!(
            "{:<36}  {}{:<20}{}  {:<12}  {}",
            run.id,
            super::status_color(&run.status),
            run.status,
            style::RESET,
            created,
            goal,
        );
    }

    Ok(())
}

fn parse_status(s: &str) -> Result<CouncilRunStatus> {
    match s {
        "running" => Ok(CouncilRunStatus::Running),
        "awaiting_approval" => Ok(CouncilRunStatus::AwaitingApproval),
        "interrupted" => Ok(CouncilRunStatus::Interrupted),
        "completed" => Ok(CouncilRunStatus::Completed),
        "failed" => Ok(CouncilRunStatus::Failed),
        other => Err(anyhow!(
            "unknown status: '{other}'. \
             Valid values: running, awaiting_approval, interrupted, completed, failed"
        )),
    }
}
