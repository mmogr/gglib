//! Static ANSI rendering for council summary and agent cards.
//!
//! Renders a scannable table of agents with colour-coded contentiousness
//! and an optional synthesis guidance line.

use gglib_agent::council::config::{CouncilConfig, SuggestedCouncil};

use crate::presentation::style::{BOLD, DIM, RESET};

use super::stream::temperature_fg;

// ─── Public API ─────────────────────────────────────────────────────────────

/// Print a colour-coded summary table for a suggested council.
pub fn render_suggested(council: &SuggestedCouncil) {
    eprintln!("\n{BOLD}Council Suggestion  ({} agents, {} rounds){RESET}", council.agents.len(), council.rounds);
    render_agent_table(&council.agents);
    if let Some(ref guidance) = council.synthesis_guidance {
        eprintln!("  {DIM}Synthesis:{RESET} {guidance}");
    }
    eprintln!();
}

/// Print a colour-coded summary table for a loaded council config.
pub fn render_config(config: &CouncilConfig) {
    eprintln!("\n{BOLD}Council Config  ({} agents, {} rounds){RESET}", config.agents.len(), config.rounds);
    eprintln!("  {DIM}Topic:{RESET} {}", config.topic);
    render_agent_table(&config.agents);
    if let Some(ref guidance) = config.synthesis_guidance {
        eprintln!("  {DIM}Synthesis:{RESET} {guidance}");
    }
    eprintln!();
}

// ─── Internals ──────────────────────────────────────────────────────────────

fn render_agent_table(agents: &[gglib_agent::council::config::CouncilAgent]) {
    // Header
    eprintln!(
        "  {DIM}{:<4}  {:<20}  {:<6}  {:<12}  Tools{RESET}",
        "#", "Name", "Cont.", "Perspective"
    );
    eprintln!("  {DIM}{}{RESET}", "─".repeat(68));

    for (i, agent) in agents.iter().enumerate() {
        let color = temperature_fg(agent.contentiousness);
        let tools = match &agent.tool_filter {
            Some(list) if !list.is_empty() => list.join(", "),
            _ => "all".into(),
        };
        // Truncate perspective to 12 chars for the table
        let perspective: String = agent.perspective.chars().take(12).collect();
        eprintln!(
            "  {color}{:<4}{RESET}  {BOLD}{:<20}{RESET}  {color}{:<6.2}{RESET}  {DIM}{:<12}{RESET}  {DIM}{tools}{RESET}",
            i + 1,
            agent.name,
            agent.contentiousness,
            perspective,
        );
    }
}
