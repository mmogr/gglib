//! Agent state mutation logic for the council editing REPL.
//!
//! Each `apply_*` function modifies a single field on a [`CouncilAgent`] or
//! the council-level settings, returning `Ok(())` on success.

use anyhow::{Result, anyhow, bail};

use gglib_agent::council::config::{CouncilAgent, CouncilConfig, clamp_contentiousness};
use gglib_agent::council::parse_tool_filter;

use crate::presentation::style::{BOLD, DIM, RESET};

use super::stream::temperature_fg;

// ─── Agent-level edits ──────────────────────────────────────────────────────

/// Rename an agent.
pub fn apply_name(agent: &mut CouncilAgent, new_name: &str) -> Result<()> {
    if new_name.is_empty() {
        bail!("name cannot be empty");
    }
    agent.name = new_name.to_owned();
    Ok(())
}

/// Set the persona (system prompt flavour) for a single agent.
pub fn apply_persona(agent: &mut CouncilAgent, new_persona: &str) -> Result<()> {
    if new_persona.is_empty() {
        bail!("persona cannot be empty");
    }
    agent.persona = new_persona.to_owned();
    Ok(())
}

/// Set the contentiousness (0.0–1.0) for a single agent.
pub fn apply_contentiousness(agent: &mut CouncilAgent, input: &str) -> Result<()> {
    let val: f32 = input
        .trim()
        .parse()
        .map_err(|_| anyhow!("expected a number between 0.0 and 1.0"))?;
    agent.contentiousness = clamp_contentiousness(val);
    Ok(())
}

/// Set the tool filter for a single agent.
///
/// Delegates to [`parse_tool_filter`] for the full supported syntax:
/// exact names, 1-based numeric indices, `N:M` ranges, and `!`-prefixed
/// exclusions.  Passing `"all"` or an empty string clears the filter.
pub fn apply_tool_filter(
    agent: &mut CouncilAgent,
    input: &str,
    available: &[String],
) -> Result<()> {
    agent.tool_filter = parse_tool_filter(input, available)?;
    Ok(())
}

// ─── Council-level edits ────────────────────────────────────────────────────

/// Set the number of deliberation rounds.
pub fn apply_rounds(config: &mut CouncilConfig, input: &str) -> Result<()> {
    let val: u32 = input
        .trim()
        .parse()
        .map_err(|_| anyhow!("expected a positive integer"))?;
    if val == 0 {
        bail!("rounds must be at least 1");
    }
    config.rounds = val;
    Ok(())
}

/// Remove an agent by 1-based index.
pub fn remove_agent(config: &mut CouncilConfig, input: &str) -> Result<()> {
    let idx: usize = input
        .trim()
        .parse::<usize>()
        .map_err(|_| anyhow!("expected an agent number"))?;
    if idx == 0 || idx > config.agents.len() {
        bail!("agent number must be between 1 and {}", config.agents.len());
    }
    let removed = config.agents.remove(idx - 1);
    eprintln!("  {DIM}Removed {}{RESET}", removed.name);
    Ok(())
}

// ─── Tool listing helper ────────────────────────────────────────────────────

/// Print a numbered list of available tools to stderr.
pub fn print_available_tools(available: &[String]) {
    eprintln!("\n  {BOLD}Available tools:{RESET}");
    for (i, name) in available.iter().enumerate() {
        eprintln!("    {DIM}{:>3}.{RESET} {name}", i + 1);
    }
    eprintln!();
}

/// Print the current state of a single agent after an edit.
pub fn print_agent_summary(idx: usize, agent: &CouncilAgent) {
    let color = temperature_fg(agent.contentiousness);
    let tools = match &agent.tool_filter {
        Some(list) if !list.is_empty() => list.join(", "),
        _ => "all".into(),
    };
    eprintln!(
        "  {color}#{}{RESET}  {BOLD}{}{RESET}  cont={color}{:.2}{RESET}  tools={DIM}{tools}{RESET}",
        idx + 1,
        agent.name,
        agent.contentiousness,
    );
}
