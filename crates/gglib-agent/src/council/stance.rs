//! Post-debate stance tracking — LLM-driven evaluation of how each agent's
//! position evolved from their initial claim to their final claim.
//!
//! After the debate rounds complete (and before synthesis), the orchestrator
//! calls [`evaluate_stances`] to produce a [`StanceMap`] — a per-agent
//! classification of whether each agent **Held**, **Shifted**, or
//! **Conceded** their original position.
//!
//! # DRY state
//!
//! This module does **not** duplicate `core_claim` data into a new index.
//! Instead, [`gather_claim_pairs`] iterates over the existing contributions
//! in [`CouncilState`] to extract the first and final core claims per agent.
//!
//! # Robust parsing
//!
//! The LLM's output must contain `STANCE(Agent Name): Held|Shifted|Conceded`
//! lines.  [`parse_stances`] uses case-insensitive, whitespace-tolerant
//! matching identical to the compaction/judge parsers — stripping markdown
//! wrapping, normalising whitespace, and falling back gracefully when lines
//! are unparseable.

use std::collections::{HashMap, HashSet};
use std::fmt::Write;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{debug, warn};

use gglib_core::{
    AGENT_EVENT_CHANNEL_CAPACITY, AgentConfig, AgentEvent, AgentMessage, LlmCompletionPort,
    ToolExecutorPort,
};

use crate::AgentLoop;

use super::events::CouncilEvent;
use super::prompts::STANCE_PROMPT;
use super::state::CouncilState;

// ─── types ───────────────────────────────────────────────────────────────────

/// How an agent's position evolved during the debate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StanceTrajectory {
    /// Position substantively unchanged from initial claim.
    Held,
    /// Position materially changed but not fully reversed.
    Shifted,
    /// Agent abandoned their initial position entirely.
    Conceded,
}

impl StanceTrajectory {
    /// Human-readable label for display.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Held => "Held",
            Self::Shifted => "Shifted",
            Self::Conceded => "Conceded",
        }
    }
}

/// Per-agent stance evaluation result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStance {
    pub agent_name: String,
    pub trajectory: StanceTrajectory,
}

/// A map of agent name → stance trajectory, ordered by insertion.
pub type StanceMap = Vec<AgentStance>;

// ─── claim pair extraction (DRY — reads from existing contributions) ─────────

/// A pair of initial and final core claims for a single agent.
#[derive(Debug)]
pub(crate) struct ClaimPair {
    pub agent_name: String,
    pub initial: Option<String>,
    pub r#final: Option<String>,
}

/// Extract the first and final core claims for each agent from the
/// existing contributions in `state`.
///
/// This avoids duplicating claim data into a separate index — it scans
/// the contributions vec directly, which is small (agents × rounds).
#[must_use]
pub(crate) fn gather_claim_pairs(state: &CouncilState) -> Vec<ClaimPair> {
    // Collect unique agent names in order of first appearance.
    let mut seen = HashSet::new();
    let mut agent_order: Vec<String> = Vec::new();

    // first claim and final claim per agent
    let mut first: HashMap<&str, String> = HashMap::new();
    let mut last: HashMap<&str, String> = HashMap::new();

    for c in state.all_contributions() {
        let name = c.agent.name.as_str();
        if seen.insert(name.to_owned()) {
            agent_order.push(name.to_owned());
        }
        if let Some(ref claim) = c.core_claim {
            first.entry(name).or_insert_with(|| claim.clone());
            last.insert(name, claim.clone());
        }
    }

    agent_order
        .into_iter()
        .map(|name| {
            let initial = first.get(name.as_str()).cloned();
            let fin = last.get(name.as_str()).cloned();
            ClaimPair {
                agent_name: name,
                initial,
                r#final: fin,
            }
        })
        .collect()
}

/// Format claim pairs into the block that gets injected into the prompt.
fn format_claims_block(pairs: &[ClaimPair]) -> String {
    let mut out = String::new();
    for p in pairs {
        let _ = writeln!(out, "Agent: {}", p.agent_name);
        match &p.initial {
            Some(c) => {
                let _ = writeln!(out, "  Initial claim: \"{c}\"");
            }
            None => {
                let _ = writeln!(out, "  Initial claim: (none stated)");
            }
        }
        match &p.r#final {
            Some(c) => {
                let _ = writeln!(out, "  Final claim: \"{c}\"");
            }
            None => {
                let _ = writeln!(out, "  Final claim: (none stated)");
            }
        }
        let _ = writeln!(out);
    }
    out
}

// ─── LLM evaluation ─────────────────────────────────────────────────────────

/// Run the stance evaluation pass after all debate rounds complete.
///
/// Makes a single bulk LLM call with all agents' initial and final claims,
/// parses the response into a [`StanceMap`], and emits a
/// [`CouncilEvent::StanceMap`] event.
///
/// If no agents have any core claims, or parsing fails entirely, the step
/// is silently skipped — stance tracking is informational, not critical.
pub(super) async fn evaluate_stances(
    state: &CouncilState,
    llm: &Arc<dyn LlmCompletionPort>,
    tool_executor: &Arc<dyn ToolExecutorPort>,
    council_tx: &mpsc::Sender<CouncilEvent>,
    topic: &str,
) {
    let pairs = gather_claim_pairs(state);

    // If no agent ever produced a core claim, skip entirely.
    if pairs
        .iter()
        .all(|p| p.initial.is_none() && p.r#final.is_none())
    {
        debug!("no core claims found — skipping stance evaluation");
        return;
    }

    let claims_block = format_claims_block(&pairs);

    #[allow(clippy::literal_string_with_formatting_args)]
    let system = STANCE_PROMPT
        .replace("{topic}", topic)
        .replace("{claims}", &claims_block);

    let messages = vec![
        AgentMessage::System { content: system },
        AgentMessage::User {
            content: "Evaluate how each agent's stance evolved during this debate.".into(),
        },
    ];

    let agent = AgentLoop::build(
        Arc::clone(llm),
        Arc::clone(tool_executor),
        Some(HashSet::new()),
    );
    let mut config = AgentConfig::default();
    config.max_iterations = 1;

    let (agent_tx, mut agent_rx) = mpsc::channel::<AgentEvent>(AGENT_EVENT_CHANNEL_CAPACITY);

    let handle = {
        let agent = Arc::clone(&agent);
        tokio::spawn(async move { agent.run(messages, config, agent_tx).await })
    };

    let mut content: Option<String> = None;
    while let Some(event) = agent_rx.recv().await {
        if let AgentEvent::FinalAnswer { content: answer } = event {
            content = Some(answer);
        }
    }

    let _ = handle.await;

    let raw = content.unwrap_or_default();
    if raw.is_empty() {
        warn!("stance evaluation agent produced no output");
        return;
    }

    let agent_names: Vec<&str> = pairs.iter().map(|p| p.agent_name.as_str()).collect();
    let stances = parse_stances(&raw, &agent_names);

    if stances.is_empty() {
        warn!("stance evaluation produced no parseable results");
        return;
    }

    debug!(count = stances.len(), "stance evaluation complete");

    let _ = council_tx
        .send(CouncilEvent::StanceMap {
            stances: stances.clone(),
        })
        .await;
}

// ─── robust parsing ─────────────────────────────────────────────────────────

/// Parse `STANCE(Agent Name): Held|Shifted|Conceded` lines from LLM output.
///
/// Uses the same robust techniques as `compaction::parse_compacted_summaries`:
/// - Case-insensitive matching of the `STANCE` keyword
/// - Strips markdown bold/backtick wrapping (`**`, `` ` ``)
/// - Tolerates extra whitespace around the colon
/// - Validates agent names against the known list
/// - Falls back to `[Agent]: Trajectory` bracket format
#[must_use]
pub(crate) fn parse_stances(raw: &str, agent_names: &[&str]) -> StanceMap {
    let mut results = Vec::new();
    let mut matched_names: HashSet<String> = HashSet::new();

    for line in raw.lines() {
        if let Some(stance) = extract_stance_line(line, agent_names) {
            let key = stance.agent_name.to_lowercase();
            if matched_names.insert(key) {
                results.push(stance);
            }
        }
    }

    // Fallback: try bracket format [Agent]: Trajectory
    if results.is_empty() {
        for line in raw.lines() {
            if let Some(stance) = extract_bracket_stance(line, agent_names) {
                let key = stance.agent_name.to_lowercase();
                if matched_names.insert(key) {
                    results.push(stance);
                }
            }
        }
    }

    results
}

/// Try to extract a `STANCE(Name): Trajectory` from a single line.
fn extract_stance_line(line: &str, agent_names: &[&str]) -> Option<AgentStance> {
    let cleaned = line.trim().replace("**", "").replace(['`', '*'], "");
    let lower = cleaned.to_lowercase();

    // Find "stance(" case-insensitively
    let stance_pos = lower.find("stance(")?;
    let after_paren = &cleaned[stance_pos + 7..]; // skip "stance(" (7 chars)

    // Find the closing paren
    let close_paren = after_paren.find(')')?;
    let name_raw = after_paren[..close_paren].trim();

    // Validate the name against known agents (case-insensitive)
    let matched_name = agent_names
        .iter()
        .find(|n| n.eq_ignore_ascii_case(name_raw))?;

    // Get the trajectory after the colon
    let rest = after_paren[close_paren + 1..].trim();
    let trajectory_str = rest.strip_prefix(':').unwrap_or(rest).trim();

    let trajectory = parse_trajectory(trajectory_str)?;

    Some(AgentStance {
        agent_name: (*matched_name).to_owned(),
        trajectory,
    })
}

/// Fallback parser: `[Agent Name]: Trajectory`
fn extract_bracket_stance(line: &str, agent_names: &[&str]) -> Option<AgentStance> {
    let cleaned = line.trim().replace("**", "").replace(['`', '*'], "");
    let trimmed = cleaned.trim();

    if !trimmed.starts_with('[') {
        return None;
    }

    let close_bracket = trimmed.find(']')?;
    let name_raw = trimmed[1..close_bracket].trim();

    let matched_name = agent_names
        .iter()
        .find(|n| n.eq_ignore_ascii_case(name_raw))?;

    let rest = trimmed[close_bracket + 1..].trim();
    let trajectory_str = rest.strip_prefix(':').unwrap_or(rest).trim();

    let trajectory = parse_trajectory(trajectory_str)?;

    Some(AgentStance {
        agent_name: (*matched_name).to_owned(),
        trajectory,
    })
}

/// Parse a trajectory keyword from text that may contain trailing prose.
fn parse_trajectory(s: &str) -> Option<StanceTrajectory> {
    let lower = s.to_lowercase();
    // Check prefix to tolerate trailing explanation (e.g. "Held — position unchanged")
    if lower.starts_with("held") {
        Some(StanceTrajectory::Held)
    } else if lower.starts_with("shifted") {
        Some(StanceTrajectory::Shifted)
    } else if lower.starts_with("conceded") {
        Some(StanceTrajectory::Conceded)
    } else {
        None
    }
}

// ─── tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::council::config::CouncilAgent;
    use crate::council::state::AgentContribution;

    fn agent(id: &str, name: &str, contentiousness: f32) -> CouncilAgent {
        CouncilAgent {
            id: id.into(),
            name: name.into(),
            color: "#000".into(),
            persona: "Test persona.".into(),
            perspective: "Test perspective.".into(),
            contentiousness,
            tool_filter: None,
        }
    }

    // ── gather_claim_pairs ───────────────────────────────────────────────

    #[test]
    fn gather_claims_two_rounds() {
        let mut state = CouncilState::new();
        state.push(AgentContribution {
            agent: agent("s", "Skeptic", 0.7),
            content: "Round 0 text.\nCORE CLAIM: Monoliths are better.".into(),
            core_claim: Some("Monoliths are better.".into()),
            round: 0,
        });
        state.push(AgentContribution {
            agent: agent("p", "Pragmatist", 0.3),
            content: "Round 0 text.\nCORE CLAIM: Use what works.".into(),
            core_claim: Some("Use what works.".into()),
            round: 0,
        });
        state.advance_round();
        state.push(AgentContribution {
            agent: agent("s", "Skeptic", 0.7),
            content: "Round 1 text.\nCORE CLAIM: Monoliths scale better for small teams.".into(),
            core_claim: Some("Monoliths scale better for small teams.".into()),
            round: 1,
        });
        state.push(AgentContribution {
            agent: agent("p", "Pragmatist", 0.3),
            content: "Round 1 text.\nCORE CLAIM: Pragmatic choice depends on team size.".into(),
            core_claim: Some("Pragmatic choice depends on team size.".into()),
            round: 1,
        });
        state.advance_round();

        let pairs = gather_claim_pairs(&state);
        assert_eq!(pairs.len(), 2);

        assert_eq!(pairs[0].agent_name, "Skeptic");
        assert_eq!(pairs[0].initial.as_deref(), Some("Monoliths are better."));
        assert_eq!(
            pairs[0].r#final.as_deref(),
            Some("Monoliths scale better for small teams.")
        );

        assert_eq!(pairs[1].agent_name, "Pragmatist");
        assert_eq!(pairs[1].initial.as_deref(), Some("Use what works."));
        assert_eq!(
            pairs[1].r#final.as_deref(),
            Some("Pragmatic choice depends on team size.")
        );
    }

    #[test]
    fn gather_claims_missing_some() {
        let mut state = CouncilState::new();
        state.push(AgentContribution {
            agent: agent("s", "Skeptic", 0.7),
            content: "No claim here.".into(),
            core_claim: None,
            round: 0,
        });
        state.push(AgentContribution {
            agent: agent("p", "Pragmatist", 0.3),
            content: "Has a claim.\nCORE CLAIM: One claim only.".into(),
            core_claim: Some("One claim only.".into()),
            round: 0,
        });
        state.advance_round();

        let pairs = gather_claim_pairs(&state);
        assert_eq!(pairs.len(), 2);

        // Skeptic has no claims at all
        assert!(pairs[0].initial.is_none());
        assert!(pairs[0].r#final.is_none());

        // Pragmatist: initial == final (only one claim)
        assert_eq!(pairs[1].initial.as_deref(), Some("One claim only."));
        assert_eq!(pairs[1].r#final.as_deref(), Some("One claim only."));
    }

    #[test]
    fn gather_claims_empty_state() {
        let state = CouncilState::new();
        let pairs = gather_claim_pairs(&state);
        assert!(pairs.is_empty());
    }

    // ── format_claims_block ──────────────────────────────────────────────

    #[test]
    fn format_claims_block_output() {
        let pairs = vec![
            ClaimPair {
                agent_name: "Skeptic".into(),
                initial: Some("Bad idea.".into()),
                r#final: Some("Maybe okay for small teams.".into()),
            },
            ClaimPair {
                agent_name: "Optimist".into(),
                initial: None,
                r#final: Some("Great idea.".into()),
            },
        ];
        let block = format_claims_block(&pairs);
        assert!(block.contains("Agent: Skeptic"));
        assert!(block.contains("Initial claim: \"Bad idea.\""));
        assert!(block.contains("Final claim: \"Maybe okay for small teams.\""));
        assert!(block.contains("Agent: Optimist"));
        assert!(block.contains("Initial claim: (none stated)"));
        assert!(block.contains("Final claim: \"Great idea.\""));
    }

    // ── parse_stances ────────────────────────────────────────────────────

    #[test]
    fn parse_clean_output() {
        let raw = "\
STANCE(Skeptic): Held
STANCE(Pragmatist): Shifted
STANCE(Optimist): Conceded";
        let names = ["Skeptic", "Pragmatist", "Optimist"];
        let stances = parse_stances(raw, &names);
        assert_eq!(stances.len(), 3);
        assert_eq!(stances[0].agent_name, "Skeptic");
        assert_eq!(stances[0].trajectory, StanceTrajectory::Held);
        assert_eq!(stances[1].agent_name, "Pragmatist");
        assert_eq!(stances[1].trajectory, StanceTrajectory::Shifted);
        assert_eq!(stances[2].agent_name, "Optimist");
        assert_eq!(stances[2].trajectory, StanceTrajectory::Conceded);
    }

    #[test]
    fn parse_with_markdown_wrapping() {
        let raw = "**STANCE(Skeptic):** Held\n`STANCE(Pragmatist):` Shifted";
        let names = ["Skeptic", "Pragmatist"];
        let stances = parse_stances(raw, &names);
        assert_eq!(stances.len(), 2);
        assert_eq!(stances[0].trajectory, StanceTrajectory::Held);
        assert_eq!(stances[1].trajectory, StanceTrajectory::Shifted);
    }

    #[test]
    fn parse_case_insensitive() {
        let raw = "stance(skeptic): held\nStAnCe(Pragmatist): SHIFTED";
        let names = ["Skeptic", "Pragmatist"];
        let stances = parse_stances(raw, &names);
        assert_eq!(stances.len(), 2);
        assert_eq!(stances[0].agent_name, "Skeptic"); // canonical name
        assert_eq!(stances[0].trajectory, StanceTrajectory::Held);
        assert_eq!(stances[1].trajectory, StanceTrajectory::Shifted);
    }

    #[test]
    fn parse_with_trailing_explanation() {
        let raw = "STANCE(Skeptic): Held — position unchanged throughout\n\
                   STANCE(Pragmatist): Shifted (moved from strong support to moderate)";
        let names = ["Skeptic", "Pragmatist"];
        let stances = parse_stances(raw, &names);
        assert_eq!(stances.len(), 2);
        assert_eq!(stances[0].trajectory, StanceTrajectory::Held);
        assert_eq!(stances[1].trajectory, StanceTrajectory::Shifted);
    }

    #[test]
    fn parse_extra_whitespace() {
        let raw = "  STANCE( Skeptic )  :  Held  \n  STANCE(  Pragmatist  ):   Conceded  ";
        let names = ["Skeptic", "Pragmatist"];
        let stances = parse_stances(raw, &names);
        assert_eq!(stances.len(), 2);
        assert_eq!(stances[0].trajectory, StanceTrajectory::Held);
        assert_eq!(stances[1].trajectory, StanceTrajectory::Conceded);
    }

    #[test]
    fn parse_unknown_agent_ignored() {
        let raw = "STANCE(Skeptic): Held\nSTANCE(Unknown): Shifted";
        let names = ["Skeptic", "Pragmatist"];
        let stances = parse_stances(raw, &names);
        assert_eq!(stances.len(), 1);
        assert_eq!(stances[0].agent_name, "Skeptic");
    }

    #[test]
    fn parse_invalid_trajectory_ignored() {
        let raw = "STANCE(Skeptic): Held\nSTANCE(Pragmatist): Flipped";
        let names = ["Skeptic", "Pragmatist"];
        let stances = parse_stances(raw, &names);
        assert_eq!(stances.len(), 1);
        assert_eq!(stances[0].agent_name, "Skeptic");
    }

    #[test]
    fn parse_duplicate_agent_takes_first() {
        let raw = "STANCE(Skeptic): Held\nSTANCE(Skeptic): Shifted";
        let names = ["Skeptic"];
        let stances = parse_stances(raw, &names);
        assert_eq!(stances.len(), 1);
        assert_eq!(stances[0].trajectory, StanceTrajectory::Held);
    }

    #[test]
    fn parse_empty_input() {
        let stances = parse_stances("", &["Skeptic"]);
        assert!(stances.is_empty());
    }

    #[test]
    fn parse_no_stance_lines() {
        let raw = "The debate was interesting.\nAll agents performed well.";
        let stances = parse_stances(raw, &["Skeptic", "Pragmatist"]);
        assert!(stances.is_empty());
    }

    // ── bracket fallback ─────────────────────────────────────────────────

    #[test]
    fn parse_bracket_fallback() {
        let raw = "[Skeptic]: Held\n[Pragmatist]: Conceded";
        let names = ["Skeptic", "Pragmatist"];
        let stances = parse_stances(raw, &names);
        assert_eq!(stances.len(), 2);
        assert_eq!(stances[0].agent_name, "Skeptic");
        assert_eq!(stances[0].trajectory, StanceTrajectory::Held);
        assert_eq!(stances[1].trajectory, StanceTrajectory::Conceded);
    }

    #[test]
    fn bracket_not_used_when_primary_succeeds() {
        // Primary format succeeds for Skeptic, bracket exists for Pragmatist.
        // But bracket is only tried when primary finds ZERO results.
        let raw = "STANCE(Skeptic): Held\n[Pragmatist]: Conceded";
        let names = ["Skeptic", "Pragmatist"];
        let stances = parse_stances(raw, &names);
        // Primary finds Skeptic → bracket not tried → Pragmatist missing
        assert_eq!(stances.len(), 1);
        assert_eq!(stances[0].agent_name, "Skeptic");
    }

    // ── trajectory parsing ───────────────────────────────────────────────

    #[test]
    fn trajectory_held() {
        assert_eq!(parse_trajectory("Held"), Some(StanceTrajectory::Held));
        assert_eq!(parse_trajectory("held"), Some(StanceTrajectory::Held));
        assert_eq!(
            parse_trajectory("Held — unchanged"),
            Some(StanceTrajectory::Held)
        );
    }

    #[test]
    fn trajectory_shifted() {
        assert_eq!(parse_trajectory("Shifted"), Some(StanceTrajectory::Shifted));
        assert_eq!(
            parse_trajectory("shifted position"),
            Some(StanceTrajectory::Shifted)
        );
    }

    #[test]
    fn trajectory_conceded() {
        assert_eq!(
            parse_trajectory("Conceded"),
            Some(StanceTrajectory::Conceded)
        );
        assert_eq!(
            parse_trajectory("CONCEDED"),
            Some(StanceTrajectory::Conceded)
        );
    }

    #[test]
    fn trajectory_invalid() {
        assert_eq!(parse_trajectory("Flipped"), None);
        assert_eq!(parse_trajectory(""), None);
        assert_eq!(parse_trajectory("   "), None);
    }

    // ── StanceTrajectory label ───────────────────────────────────────────

    #[test]
    fn trajectory_labels() {
        assert_eq!(StanceTrajectory::Held.label(), "Held");
        assert_eq!(StanceTrajectory::Shifted.label(), "Shifted");
        assert_eq!(StanceTrajectory::Conceded.label(), "Conceded");
    }

    // ── serde round-trip ─────────────────────────────────────────────────

    #[test]
    fn agent_stance_serde_round_trip() {
        let stance = AgentStance {
            agent_name: "Skeptic".into(),
            trajectory: StanceTrajectory::Shifted,
        };
        let json = serde_json::to_string(&stance).unwrap();
        let back: AgentStance = serde_json::from_str(&json).unwrap();
        assert_eq!(back.agent_name, "Skeptic");
        assert_eq!(back.trajectory, StanceTrajectory::Shifted);
    }

    #[test]
    fn trajectory_serializes_snake_case() {
        let json = serde_json::to_string(&StanceTrajectory::Held).unwrap();
        assert_eq!(json, "\"held\"");
        let json = serde_json::to_string(&StanceTrajectory::Shifted).unwrap();
        assert_eq!(json, "\"shifted\"");
        let json = serde_json::to_string(&StanceTrajectory::Conceded).unwrap();
        assert_eq!(json, "\"conceded\"");
    }
}
