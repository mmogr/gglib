//! Per-agent context assembly for council debate turns.
//!
//! Builds the full `Vec<AgentMessage>` for a single agent turn by:
//!
//! 1. Constructing a system prompt with identity anchoring (agent name,
//!    persona, contentiousness instruction — re-injected every turn).
//! 2. Formatting the debate transcript from prior rounds as a labelled
//!    `[Agent Name]: content` block.
//! 3. Appending round-phase suffixes (debate-history cue, final-round cue).
//! 4. Wrapping the topic as a `User` message.

use gglib_core::AgentMessage;

use super::config::CouncilAgent;
use super::prompts::{
    AGENT_TURN_SYSTEM_PROMPT, DEBATE_HISTORY_SUFFIX, FINAL_ROUND_SUFFIX,
    contentiousness_to_instruction,
};
use super::state::CouncilState;

/// Build the message list for a single agent's turn in the council debate.
///
/// Returns `[System(prompt), User(topic)]` — a two-message conversation
/// that the `AgentLoop` will extend with tool calls and responses during
/// its own run.
///
/// # Arguments
///
/// - `agent` — the agent about to speak.
/// - `topic` — the user's original question/topic.
/// - `round` — zero-indexed current round.
/// - `total_rounds` — total debate rounds (used for final-round detection).
/// - `state` — accumulated contributions from prior turns/rounds.
#[must_use]
pub fn build_agent_messages(
    agent: &CouncilAgent,
    topic: &str,
    round: u32,
    total_rounds: u32,
    state: &CouncilState,
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
/// This is separated from [`build_agent_messages`] for testability.
#[must_use]
pub fn build_agent_system_prompt(
    agent: &CouncilAgent,
    topic: &str,
    round: u32,
    total_rounds: u32,
    state: &CouncilState,
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
            prompt.push_str(DEBATE_HISTORY_SUFFIX);
        }
    }

    // Final-round cue.
    let is_final = total_rounds > 0 && round == total_rounds - 1;
    if is_final {
        prompt.push_str(FINAL_ROUND_SUFFIX);
    }

    prompt
}

/// Format prior contributions as a labelled transcript block.
///
/// Output format:
/// ```text
/// === Round 1 ===
/// [Skeptic]: Their argument text...
/// [Pragmatist]: Their argument text...
/// === Round 2 ===
/// ...
/// ```
///
/// Only includes rounds `0..round` (exclusive of the current round).
#[must_use]
fn format_transcript(state: &CouncilState, up_to_round: u32) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    for r in 0..up_to_round {
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
pub fn format_synthesis_transcript(state: &CouncilState) -> String {
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
    use crate::council::config::CouncilAgent;
    use crate::council::state::AgentContribution;

    fn agent(id: &str, name: &str, contentiousness: f32) -> CouncilAgent {
        CouncilAgent {
            id: id.into(),
            name: name.into(),
            color: "#000".into(),
            persona: format!("{name} is a test agent."),
            perspective: format!("{name}'s angle"),
            contentiousness,
            tool_filter: None,
        }
    }

    #[test]
    fn round_0_no_history() {
        let state = CouncilState::new();
        let a = agent("s", "Skeptic", 0.7);
        let prompt = build_agent_system_prompt(&a, "Test topic", 0, 3, &state);

        assert!(prompt.contains("You are Skeptic."));
        assert!(prompt.contains("Skeptic is a test agent."));
        assert!(prompt.contains("rigorous critic"));
        assert!(!prompt.contains("DEBATE HISTORY"));
        assert!(!prompt.contains("FINAL ROUND"));
    }

    #[test]
    fn round_1_includes_history() {
        let mut state = CouncilState::new();
        state.push(AgentContribution {
            agent: agent("s", "Skeptic", 0.7),
            content: "I disagree strongly.".into(),
            core_claim: Some("Bad idea.".into()),
            round: 0,
        });
        state.push(AgentContribution {
            agent: agent("p", "Pragmatist", 0.3),
            content: "Let's find middle ground.".into(),
            core_claim: None,
            round: 0,
        });
        state.advance_round();

        let a = agent("s", "Skeptic", 0.7);
        let prompt = build_agent_system_prompt(&a, "Test topic", 1, 3, &state);

        assert!(prompt.contains("DEBATE HISTORY:"));
        assert!(prompt.contains("=== Round 1 ==="));
        assert!(prompt.contains("[Skeptic]: I disagree strongly."));
        assert!(prompt.contains("[Pragmatist]: Let's find middle ground."));
        assert!(prompt.contains("Respond to the strongest counterarguments"));
        assert!(!prompt.contains("FINAL ROUND"));
    }

    #[test]
    fn final_round_suffix_appended() {
        let state = CouncilState::new();
        let a = agent("s", "Skeptic", 0.7);
        let prompt = build_agent_system_prompt(&a, "Topic", 2, 3, &state);
        assert!(prompt.contains("FINAL ROUND"));
    }

    #[test]
    fn single_round_is_also_final() {
        let state = CouncilState::new();
        let a = agent("s", "Skeptic", 0.7);
        let prompt = build_agent_system_prompt(&a, "Topic", 0, 1, &state);
        assert!(prompt.contains("FINAL ROUND"));
    }

    #[test]
    fn build_agent_messages_structure() {
        let state = CouncilState::new();
        let a = agent("s", "Skeptic", 0.7);
        let msgs = build_agent_messages(&a, "My topic", 0, 2, &state);

        assert_eq!(msgs.len(), 2);
        assert!(
            matches!(&msgs[0], AgentMessage::System { content } if content.contains("Skeptic"))
        );
        assert!(matches!(&msgs[1], AgentMessage::User { content } if content == "My topic"));
    }

    #[test]
    fn synthesis_transcript_includes_perspectives() {
        let mut state = CouncilState::new();
        state.push(AgentContribution {
            agent: agent("s", "Skeptic", 0.7),
            content: "Bad idea.".into(),
            core_claim: None,
            round: 0,
        });
        let transcript = format_synthesis_transcript(&state);
        assert!(transcript.contains("[Skeptic (Skeptic's angle)]: Bad idea."));
    }
}
