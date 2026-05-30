//! Post-debate stance evaluation.
//!
//! After all debate rounds complete, a single LLM call evaluates how each
//! agent's position evolved across rounds, then emits a
//! [`CouncilEvent::DebateStanceMap`] event with per-agent outcomes.
//!
//! # Robust parsing
//!
//! The evaluator's output must contain `STANCE(Agent Name): Held|Shifted|Conceded`
//! lines.  Parsing is case-insensitive, whitespace-tolerant, and strips
//! markdown formatting artefacts.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use tokio::sync::mpsc;
use tracing::{debug, warn};

use gglib_core::domain::council::events::{AgentStance, CouncilEvent, StanceOutcome};
use gglib_core::ports::{LlmCompletionPort, ToolExecutorPort};
use gglib_core::{AGENT_EVENT_CHANNEL_CAPACITY, AgentConfig, AgentEvent, AgentMessage};

use crate::AgentLoop;

use super::prompts::STANCE_PROMPT;
use super::state::DebateState;

// ─── internal stance types ───────────────────────────────────────────────────

/// Local claim-pair representation used for building the prompt.
struct ClaimPair {
    agent_name: String,
    agent_id: String,
    initial: Option<String>,
    r#final: Option<String>,
}

// ─── public entry point ──────────────────────────────────────────────────────

/// Run the stance evaluation pass after all debate rounds complete.
///
/// Makes a single bulk LLM call with all agents' initial and final claims,
/// parses the response, and emits a [`CouncilEvent::DebateStanceMap`] event.
///
/// If no agents have any core claims, or parsing fails entirely, the step
/// is silently skipped — stance tracking is informational, not critical.
pub(super) async fn evaluate_stances(
    node_id: &str,
    state: &DebateState,
    llm: &Arc<dyn LlmCompletionPort>,
    tool_executor: &Arc<dyn ToolExecutorPort>,
    council_tx: &mpsc::Sender<CouncilEvent>,
    topic: &str,
) {
    let pairs = gather_claim_pairs(state);

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
    let name_to_id: HashMap<&str, &str> = pairs
        .iter()
        .map(|p| (p.agent_name.as_str(), p.agent_id.as_str()))
        .collect();

    let stances = parse_stances(&raw, &agent_names, &name_to_id);

    if stances.is_empty() {
        warn!("stance evaluation produced no parseable results");
        return;
    }

    debug!(count = stances.len(), "debate stance evaluation complete");

    let _ = council_tx
        .send(CouncilEvent::DebateStanceMap {
            node_id: node_id.to_owned(),
            stances,
        })
        .await;
}

// ─── helpers ─────────────────────────────────────────────────────────────────

/// Gather each agent's first and last core claims from the debate state.
fn gather_claim_pairs(state: &DebateState) -> Vec<ClaimPair> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut agent_order: Vec<String> = Vec::new();
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
            let id = state.agent_id_for_name(&name).unwrap_or("").to_owned();
            let initial = first.get(name.as_str()).cloned();
            let fin = last.get(name.as_str()).cloned();
            ClaimPair {
                agent_name: name,
                agent_id: id,
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
        let _ = std::fmt::write(&mut out, format_args!("Agent: {}\n", p.agent_name));
        match &p.initial {
            Some(c) => {
                let _ = std::fmt::write(&mut out, format_args!("  Initial claim: \"{c}\"\n"));
            }
            None => {
                let _ = std::fmt::write(&mut out, format_args!("  Initial claim: (none stated)\n"));
            }
        }
        match &p.r#final {
            Some(c) => {
                let _ = std::fmt::write(&mut out, format_args!("  Final claim: \"{c}\"\n"));
            }
            None => {
                let _ = std::fmt::write(&mut out, format_args!("  Final claim: (none stated)\n"));
            }
        }
        out.push('\n');
    }
    out
}

/// Parse `STANCE(Agent Name): Held|Shifted|Conceded` lines from LLM output.
fn parse_stances(
    raw: &str,
    agent_names: &[&str],
    name_to_id: &HashMap<&str, &str>,
) -> Vec<AgentStance> {
    let mut results = Vec::new();

    for line in raw.lines() {
        if let Some((name, trajectory)) = extract_stance_line(line) {
            // Find matching agent name (case-insensitive).
            if let Some(matched) = agent_names
                .iter()
                .find(|n| n.to_lowercase() == name.to_lowercase())
            {
                let agent_id = name_to_id.get(*matched).copied().unwrap_or("").to_owned();
                let outcome = trajectory;
                results.push(AgentStance { agent_id, outcome });
            }
        }
    }

    results
}

/// Try to extract a `STANCE(name): trajectory` pattern from a single line.
fn extract_stance_line(line: &str) -> Option<(String, StanceOutcome)> {
    let cleaned: String = line.chars().filter(|c| *c != '*' && *c != '`').collect();
    let lower = cleaned.to_lowercase();

    let stance_idx = lower.find("stance")?;
    let after_keyword = &cleaned[stance_idx + "stance".len()..];

    let open = after_keyword.find('(')?;
    let close = after_keyword.find(')')?;
    if close <= open {
        return None;
    }

    let name = after_keyword[open + 1..close].trim().to_owned();
    if name.is_empty() {
        return None;
    }

    let after_close = &after_keyword[close + 1..];
    let colon_idx = after_close.find(':')?;
    let value = after_close[colon_idx + 1..].trim().to_lowercase();

    let outcome = parse_trajectory(&value)?;
    Some((name, outcome))
}

/// Parse a trajectory string into a [`StanceOutcome`].
fn parse_trajectory(s: &str) -> Option<StanceOutcome> {
    if s.starts_with("held") {
        Some(StanceOutcome::Held)
    } else if s.starts_with("shifted") {
        Some(StanceOutcome::Shifted)
    } else if s.starts_with("conceded") {
        Some(StanceOutcome::Conceded)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_all_trajectories() {
        let names = ["Alice", "Bob", "Carol"];
        let name_to_id: HashMap<&str, &str> = [("Alice", "a"), ("Bob", "b"), ("Carol", "c")]
            .into_iter()
            .collect();
        let raw = "STANCE(Alice): Held\nSTANCE(Bob): Shifted\nSTANCE(Carol): Conceded";
        let stances = parse_stances(raw, &names, &name_to_id);
        assert_eq!(stances.len(), 3);
        assert!(matches!(stances[0].outcome, StanceOutcome::Held));
        assert!(matches!(stances[1].outcome, StanceOutcome::Shifted));
        assert!(matches!(stances[2].outcome, StanceOutcome::Conceded));
    }

    #[test]
    fn parse_case_insensitive_trajectory() {
        let names = ["Alice"];
        let name_to_id: HashMap<&str, &str> = [("Alice", "a")].into_iter().collect();
        let raw = "STANCE(Alice): HELD";
        let stances = parse_stances(raw, &names, &name_to_id);
        assert_eq!(stances.len(), 1);
        assert!(matches!(stances[0].outcome, StanceOutcome::Held));
    }

    #[test]
    fn unknown_agent_name_skipped() {
        let names = ["Alice"];
        let name_to_id: HashMap<&str, &str> = [("Alice", "a")].into_iter().collect();
        let raw = "STANCE(Attacker): Held";
        let stances = parse_stances(raw, &names, &name_to_id);
        assert_eq!(stances.len(), 0);
    }

    #[test]
    fn markdown_wrapped_stance() {
        let names = ["Skeptic"];
        let name_to_id: HashMap<&str, &str> = [("Skeptic", "s")].into_iter().collect();
        let raw = "**STANCE(Skeptic):** Shifted";
        let stances = parse_stances(raw, &names, &name_to_id);
        assert_eq!(stances.len(), 1);
        assert!(matches!(stances[0].outcome, StanceOutcome::Shifted));
    }

    #[test]
    fn empty_output_returns_empty() {
        let names: &[&str] = &["Alice"];
        let name_to_id: HashMap<&str, &str> = HashMap::new();
        let stances = parse_stances("", names, &name_to_id);
        assert!(stances.is_empty());
    }
}
