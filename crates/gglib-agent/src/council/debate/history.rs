//! Per-agent context assembly for debate turns.
//!
//! Builds the full `Vec<AgentMessage>` for a single agent turn by:
//!
//! 1. Constructing a system prompt with identity anchoring (agent name,
//!    persona, contentiousness instruction — re-injected every turn).
//! 2. Formatting the debate transcript from prior rounds as a labelled
//!    `[Agent Name]: content` block.  Rounds that have been compacted are
//!    replaced with their short summary, keeping context sizes manageable.
//! 3. Appending a guided rebuttal cue that lets the agent autonomously
//!    choose which prior argument to rebut based on genuine conflict.
//! 4. Appending round-phase suffixes (rebuttal cue, final-round cue).
//! 5. Wrapping the topic as a `User` message.

use gglib_core::AgentMessage;
use gglib_core::domain::council::task_graph::DebateAgent;

use super::prompts::{
    AGENT_TURN_SYSTEM_PROMPT, FINAL_ROUND_SUFFIX, GUIDED_REBUTTAL_CUE,
    contentiousness_to_instruction,
};
use super::state::DebateState;

/// Build the message list for a single agent's turn in a debate node.
///
/// Returns `[System(prompt), User(topic)]` that the `AgentLoop` will extend
/// with tool calls and responses during its own run.
///
/// # Arguments
///
/// - `agent` — the agent about to speak.
/// - `topic` — the debate topic (from the node goal).
/// - `round` — zero-indexed current round.
/// - `total_rounds` — total debate rounds (used for final-round detection).
/// - `state` — accumulated contributions from prior turns/rounds.
#[must_use]
pub fn build_agent_messages(
    agent: &DebateAgent,
    topic: &str,
    round: u32,
    total_rounds: u32,
    state: &DebateState,
) -> Vec<AgentMessage> {
    let system_prompt = build_agent_system_prompt(agent, topic, round, total_rounds, state);
    vec![
        AgentMessage::System {
            content: system_prompt,
        },
        AgentMessage::User {
            content: topic.to_owned(),
        },
    ]
}

/// Assemble the full system prompt for a single agent turn.
///
/// Separated from [`build_agent_messages`] for testability.
#[must_use]
pub fn build_agent_system_prompt(
    agent: &DebateAgent,
    topic: &str,
    round: u32,
    total_rounds: u32,
    state: &DebateState,
) -> String {
    let instruction = contentiousness_to_instruction(agent.contentiousness);

    #[allow(clippy::literal_string_with_formatting_args)]
    let mut prompt = AGENT_TURN_SYSTEM_PROMPT
        .replace("{agent_name}", &agent.name)
        .replace("{agent_persona}", &agent.persona)
        .replace("{topic}", topic)
        .replace("{perspective}", &agent.perspective)
        .replace("{contentiousness_instruction}", instruction);

    // Inject debate history from prior rounds.
    if round > 0 {
        let transcript = format_transcript(state, round);
        if !transcript.is_empty() {
            prompt.push_str("\n\nDEBATE HISTORY:\n");
            prompt.push_str(&transcript);
            prompt.push_str(GUIDED_REBUTTAL_CUE);

            // Anti-dogpile: show which claims earlier agents already
            // addressed this round so this agent picks a different target.
            let already = format_already_addressed(state, round, &agent.name);
            if !already.is_empty() {
                prompt.push_str(&already);
            }
        }
    }

    // Final-round cue.
    let is_final = total_rounds > 0 && round == total_rounds - 1;
    if is_final {
        prompt.push_str(FINAL_ROUND_SUFFIX);
    }

    prompt
}

/// Summarise which prior claims earlier agents in this round have already
/// rebutted, so the current agent can pick a different target.
///
/// Returns an empty string when no earlier agents have spoken this round
/// or none of them produced a core claim.
#[must_use]
fn format_already_addressed(state: &DebateState, round: u32, self_name: &str) -> String {
    use std::fmt::Write;
    let earlier: Vec<_> = state
        .contributions_for_round(round)
        .into_iter()
        .filter(|c| c.agent.name != self_name)
        .filter_map(|c| c.core_claim.as_deref().map(|claim| (&*c.agent.name, claim)))
        .collect();

    if earlier.is_empty() {
        return String::new();
    }

    let mut out = String::from(
        "\n\nCLAIMS ALREADY ADDRESSED THIS ROUND (choose a different target if possible):\n",
    );
    for (name, claim) in &earlier {
        let _ = writeln!(out, "- {name}: \"{claim}\"");
    }
    out
}

/// Format prior contributions as a labelled transcript block.
///
/// For rounds that have been compacted, the short summary is used in
/// place of the full per-agent contributions.  The most recent round
/// (`up_to_round - 1`) is always shown in full — compaction only
/// applies to older rounds.
#[must_use]
fn format_transcript(state: &DebateState, up_to_round: u32) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    for r in 0..up_to_round {
        // Use compacted summary for older rounds if available.
        if let Some(compacted) = state.compacted_summary(r) {
            let _ = writeln!(out, "=== Round {} (compacted) ===", r + 1);
            let _ = writeln!(out, "{compacted}");
            continue;
        }

        let contributions = state.contributions_for_round(r);
        if contributions.is_empty() {
            continue;
        }
        let _ = writeln!(out, "=== Round {} ===", r + 1);
        for c in &contributions {
            let _ = writeln!(out, "[{}]: {}", c.agent.name, c.content);
        }
    }
    out
}

/// Format the full transcript for the synthesis prompt (all rounds).
///
/// Similar to [`format_transcript`] but includes agent perspectives and
/// covers all completed rounds.
#[must_use]
pub fn format_synthesis_transcript(state: &DebateState) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    for (round, contributions) in state.rounds_with_contributions() {
        let _ = writeln!(out, "=== Round {} ===", round + 1);
        for c in &contributions {
            let _ = writeln!(
                out,
                "[{} ({})]: {}",
                c.agent.name, c.agent.perspective, c.content
            );
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::council::debate::state::AgentContribution;

    fn make_agent(id: &str, name: &str) -> DebateAgent {
        DebateAgent {
            id: id.into(),
            name: name.into(),
            color: "#fff".into(),
            persona: "A thinker".into(),
            perspective: "analytical view".into(),
            contentiousness: 0.5,
            tool_filter: None,
        }
    }

    fn make_contribution(agent: DebateAgent, content: &str, round: u32) -> AgentContribution {
        AgentContribution {
            core_claim: None,
            agent,
            content: content.into(),
            round,
        }
    }

    #[test]
    fn build_messages_has_system_and_user() {
        let agent = make_agent("a1", "Alice");
        let state = DebateState::new();
        let msgs = build_agent_messages(&agent, "Is X good?", 0, 2, &state);
        assert_eq!(msgs.len(), 2);
        assert!(matches!(&msgs[0], AgentMessage::System { content } if content.contains("Alice")));
        assert!(matches!(&msgs[1], AgentMessage::User { content } if content == "Is X good?"));
    }

    #[test]
    fn no_history_injected_on_round_zero() {
        let agent = make_agent("a1", "Alice");
        let state = DebateState::new();
        let prompt = build_agent_system_prompt(&agent, "Topic", 0, 2, &state);
        assert!(!prompt.contains("DEBATE HISTORY"));
    }

    #[test]
    fn history_injected_on_round_one() {
        let agent_a = make_agent("a1", "Alice");
        let agent_b = make_agent("b1", "Bob");
        let mut state = DebateState::new();
        state.push(make_contribution(
            agent_a.clone(),
            "Alice round 0 argument.",
            0,
        ));
        state.push(make_contribution(
            agent_b.clone(),
            "Bob round 0 argument.",
            0,
        ));

        let prompt = build_agent_system_prompt(&agent_a, "Topic", 1, 2, &state);
        assert!(prompt.contains("DEBATE HISTORY"));
        assert!(prompt.contains("Alice round 0 argument."));
        assert!(prompt.contains("Bob round 0 argument."));
    }

    #[test]
    fn final_round_suffix_appended_on_last_round() {
        let agent = make_agent("a1", "Alice");
        let state = DebateState::new();
        let prompt = build_agent_system_prompt(&agent, "Topic", 1, 2, &state);
        assert!(prompt.contains("FINAL ROUND"));
    }

    #[test]
    fn no_final_round_suffix_on_non_final_round() {
        let agent = make_agent("a1", "Alice");
        let state = DebateState::new();
        let prompt = build_agent_system_prompt(&agent, "Topic", 0, 2, &state);
        assert!(!prompt.contains("FINAL ROUND"));
    }

    #[test]
    fn compacted_round_uses_summary() {
        let agent_a = make_agent("a1", "Alice");
        let agent_b = make_agent("b1", "Bob");
        let mut state = DebateState::new();
        state.push(make_contribution(agent_a.clone(), "Full text", 0));
        state.push(make_contribution(agent_b.clone(), "Full text B", 0));
        state.set_compacted(0, "[Alice]: short. [Bob]: short.".into());

        let prompt = build_agent_system_prompt(&agent_a, "Topic", 1, 3, &state);
        assert!(prompt.contains("compacted"));
        assert!(!prompt.contains("Full text"));
    }
}
