//! Per-agent context assembly for council debate turns.
//!
//! Builds the full `Vec<AgentMessage>` for a single agent turn by:
//!
//! 1. Constructing a system prompt with identity anchoring (agent name,
//!    persona, contentiousness instruction — re-injected every turn).
//! 2. Formatting the debate transcript from prior rounds as a labelled
//!    `[Agent Name]: content` block.  Rounds that have been compacted are
//!    replaced with their short summary, keeping context sizes manageable.
//! 3. Selecting a directed rebuttal target (the prior-round agent with a
//!    core claim whose contentiousness is most different from the current
//!    agent), or falling back to a generic debate-history cue.
//! 4. Appending round-phase suffixes (rebuttal/history cue, final-round cue).
//! 5. Wrapping the topic as a `User` message.

use std::path::Path;

use gglib_core::AgentMessage;

use super::config::CouncilAgent;
use super::prompts::{
    AGENT_TURN_SYSTEM_PROMPT, DEBATE_HISTORY_SUFFIX, FILESYSTEM_TOOLS_CONTEXT, FINAL_ROUND_SUFFIX,
    TARGETED_REBUTTAL_CUE, contentiousness_to_instruction,
};
use super::state::{AgentContribution, CouncilState};

/// Build the message list for a single agent's turn in the council debate.
///
/// Returns `(messages, rebuttal_target)` — the messages are
/// `[System(prompt), User(topic)]` that the `AgentLoop` will extend with
/// tool calls and responses during its own run.  `rebuttal_target` is the
/// name of the agent whose claim is being rebutted, if any.
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
    cwd: Option<&Path>,
) -> (Vec<AgentMessage>, Option<String>) {
    let (system_prompt, rebuttal_target) =
        build_agent_system_prompt(agent, topic, round, total_rounds, state, cwd);
    let messages = vec![
        AgentMessage::System {
            content: system_prompt,
        },
        AgentMessage::User {
            content: topic.to_owned(),
        },
    ];
    (messages, rebuttal_target)
}

/// Assemble the full system prompt for a single agent turn.
///
/// Returns `(prompt, rebuttal_target_name)`.  The target name is `Some`
/// only when a directed rebuttal cue was injected into the prompt.
///
/// This is separated from [`build_agent_messages`] for testability.
#[must_use]
pub fn build_agent_system_prompt(
    agent: &CouncilAgent,
    topic: &str,
    round: u32,
    total_rounds: u32,
    state: &CouncilState,
    cwd: Option<&Path>,
) -> (String, Option<String>) {
    let instruction = contentiousness_to_instruction(agent.contentiousness);

    #[allow(clippy::literal_string_with_formatting_args)]
    let mut prompt = AGENT_TURN_SYSTEM_PROMPT
        .replace("{agent_name}", &agent.name)
        .replace("{agent_persona}", &agent.persona)
        .replace("{topic}", topic)
        .replace("{perspective}", &agent.perspective)
        .replace("{contentiousness_instruction}", instruction);

    let mut rebuttal_target = None;

    // Inject debate history from prior rounds.
    if round > 0 {
        let transcript = format_transcript(state, round);
        if !transcript.is_empty() {
            prompt.push_str("\n\nDEBATE HISTORY:\n");
            prompt.push_str(&transcript);

            // Directed rebuttal cue targeting the most opposed agent's
            // core claim, or generic debate-history suffix as fallback.
            if let Some(target) = select_rebuttal_target(agent, state, round) {
                let claim = target.core_claim.as_deref().unwrap_or_default();
                prompt.push_str(
                    &TARGETED_REBUTTAL_CUE
                        .replace("{target_name}", &target.agent.name)
                        .replace("{target_claim}", claim),
                );
                rebuttal_target = Some(target.agent.name.clone());
            } else {
                prompt.push_str(DEBATE_HISTORY_SUFFIX);
            }
        }
    }

    // Final-round cue.
    let is_final = total_rounds > 0 && round == total_rounds - 1;
    if is_final {
        prompt.push_str(FINAL_ROUND_SUFFIX);
    }

    // Filesystem context — when a working directory is available, tell
    // the agent about filesystem tools so it can inspect the codebase.
    if let Some(dir) = cwd {
        use std::fmt::Write as _;
        prompt.push_str(FILESYSTEM_TOOLS_CONTEXT);
        write!(prompt, "\n\nWorking directory: {}", dir.display()).unwrap();
    }

    (prompt, rebuttal_target)
}

/// Format prior contributions as a labelled transcript block.
///
/// For rounds that have been compacted, the short summary is used in
/// place of the full per-agent contributions.  The most recent round
/// (`up_to_round - 1`) is always shown in full — compaction only
/// applies to older rounds.
///
/// Output format:
/// ```text
/// === Round 1 (compacted) ===
/// [Skeptic]: Short summary of their position.
/// [Pragmatist]: Short summary of their position.
/// === Round 2 ===
/// [Skeptic]: Their full argument text...
/// [Pragmatist]: Their full argument text...
/// ```
///
/// Only includes rounds `0..round` (exclusive of the current round).
#[must_use]
fn format_transcript(state: &CouncilState, up_to_round: u32) -> String {
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

/// Select the best rebuttal target for an agent entering a new round.
///
/// Picks the contribution from the **previous round** whose `core_claim` is
/// present and whose contentiousness is most different from the current
/// agent's — a lightweight proxy for "most semantically distant" without
/// requiring embeddings.
///
/// Returns `None` when:
/// - `round == 0` (no prior contributions exist)
/// - no other agent produced a core claim in the previous round
fn select_rebuttal_target<'a>(
    agent: &CouncilAgent,
    state: &'a CouncilState,
    round: u32,
) -> Option<&'a AgentContribution> {
    if round == 0 {
        return None;
    }
    let prev_round = round - 1;
    state
        .contributions_for_round(prev_round)
        .into_iter()
        .filter(|c| c.agent.id != agent.id && c.core_claim.is_some())
        .max_by(|a, b| {
            let dist_a = (a.agent.contentiousness - agent.contentiousness).abs();
            let dist_b = (b.agent.contentiousness - agent.contentiousness).abs();
            dist_a
                .partial_cmp(&dist_b)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
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
        let (prompt, target) = build_agent_system_prompt(&a, "Test topic", 0, 3, &state, None);

        assert!(prompt.contains("You are Skeptic."));
        assert!(prompt.contains("Skeptic is a test agent."));
        assert!(prompt.contains("rigorous critic"));
        assert!(!prompt.contains("DEBATE HISTORY"));
        assert!(!prompt.contains("FINAL ROUND"));
        assert!(target.is_none());
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
        let (prompt, _target) = build_agent_system_prompt(&a, "Test topic", 1, 3, &state, None);

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
        let (prompt, _) = build_agent_system_prompt(&a, "Topic", 2, 3, &state, None);
        assert!(prompt.contains("FINAL ROUND"));
    }

    #[test]
    fn single_round_is_also_final() {
        let state = CouncilState::new();
        let a = agent("s", "Skeptic", 0.7);
        let (prompt, _) = build_agent_system_prompt(&a, "Topic", 0, 1, &state, None);
        assert!(prompt.contains("FINAL ROUND"));
    }

    #[test]
    fn build_agent_messages_structure() {
        let state = CouncilState::new();
        let a = agent("s", "Skeptic", 0.7);
        let (msgs, target) = build_agent_messages(&a, "My topic", 0, 2, &state, None);

        assert_eq!(msgs.len(), 2);
        assert!(
            matches!(&msgs[0], AgentMessage::System { content } if content.contains("Skeptic"))
        );
        assert!(matches!(&msgs[1], AgentMessage::User { content } if content == "My topic"));
        assert!(target.is_none());
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

    // ── directed rebuttal tests ──────────────────────────────────────────

    #[test]
    fn rebuttal_target_none_at_round_0() {
        let state = CouncilState::new();
        let a = agent("s", "Skeptic", 0.7);
        assert!(select_rebuttal_target(&a, &state, 0).is_none());
    }

    #[test]
    fn rebuttal_target_picks_most_opposed() {
        let mut state = CouncilState::new();
        // Round 0: three agents with core claims
        state.push(AgentContribution {
            agent: agent("c", "Collaborator", 0.1),
            content: "We should cooperate.".into(),
            core_claim: Some("Cooperation wins.".into()),
            round: 0,
        });
        state.push(AgentContribution {
            agent: agent("b", "Balanced", 0.5),
            content: "Both sides have merit.".into(),
            core_claim: Some("Balance is key.".into()),
            round: 0,
        });
        state.push(AgentContribution {
            agent: agent("d", "Devil", 0.9),
            content: "Everything is wrong.".into(),
            core_claim: Some("Total opposition.".into()),
            round: 0,
        });
        state.advance_round();

        // Current agent is Collaborator (0.1) — most distant is Devil (0.9)
        let a = agent("c", "Collaborator", 0.1);
        let target = select_rebuttal_target(&a, &state, 1).unwrap();
        assert_eq!(target.agent.id, "d");
        assert_eq!(target.core_claim.as_deref(), Some("Total opposition."));
    }

    #[test]
    fn rebuttal_target_skips_self() {
        let mut state = CouncilState::new();
        state.push(AgentContribution {
            agent: agent("s", "Skeptic", 0.9),
            content: "My argument.".into(),
            core_claim: Some("My claim.".into()),
            round: 0,
        });
        state.advance_round();

        // Only contribution in previous round is from self — no target
        let a = agent("s", "Skeptic", 0.9);
        assert!(select_rebuttal_target(&a, &state, 1).is_none());
    }

    #[test]
    fn rebuttal_target_none_when_no_core_claims() {
        let mut state = CouncilState::new();
        state.push(AgentContribution {
            agent: agent("a", "Alice", 0.3),
            content: "Some argument.".into(),
            core_claim: None,
            round: 0,
        });
        state.push(AgentContribution {
            agent: agent("b", "Bob", 0.8),
            content: "Another argument.".into(),
            core_claim: None,
            round: 0,
        });
        state.advance_round();

        let a = agent("a", "Alice", 0.3);
        assert!(select_rebuttal_target(&a, &state, 1).is_none());
    }

    #[test]
    fn rebuttal_cue_injected_in_prompt() {
        let mut state = CouncilState::new();
        state.push(AgentContribution {
            agent: agent("s", "Skeptic", 0.9),
            content: "Bad idea.".into(),
            core_claim: Some("Monoliths scale better.".into()),
            round: 0,
        });
        state.push(AgentContribution {
            agent: agent("p", "Pragmatist", 0.2),
            content: "Let's be practical.".into(),
            core_claim: Some("Use what works.".into()),
            round: 0,
        });
        state.advance_round();

        // Pragmatist (0.2) should target Skeptic (0.9) — most distant
        let a = agent("p", "Pragmatist", 0.2);
        let (prompt, target) = build_agent_system_prompt(&a, "Architecture", 1, 3, &state, None);

        assert!(prompt.contains("DIRECTED REBUTTAL"));
        assert!(prompt.contains("Skeptic's core claim"));
        assert!(prompt.contains("Monoliths scale better."));
        // Generic suffix should NOT appear when rebuttal cue is used
        assert!(!prompt.contains("Respond to the strongest counterarguments"));
        assert_eq!(target.as_deref(), Some("Skeptic"));
    }

    #[test]
    fn generic_suffix_when_no_rebuttal_target() {
        let mut state = CouncilState::new();
        state.push(AgentContribution {
            agent: agent("a", "Alice", 0.3),
            content: "Some argument.".into(),
            core_claim: None, // no core claim
            round: 0,
        });
        state.advance_round();

        let a = agent("b", "Bob", 0.7);
        let (prompt, target) = build_agent_system_prompt(&a, "Topic", 1, 3, &state, None);

        assert!(prompt.contains("DEBATE HISTORY"));
        assert!(prompt.contains("Respond to the strongest counterarguments"));
        assert!(!prompt.contains("DIRECTED REBUTTAL"));
        assert!(target.is_none());
    }

    // ── compacted transcript tests ───────────────────────────────────────

    #[test]
    fn compacted_round_uses_summary() {
        let mut state = CouncilState::new();
        state.push(AgentContribution {
            agent: agent("s", "Skeptic", 0.7),
            content: "Very long argument about monoliths...".into(),
            core_claim: Some("Monoliths scale better.".into()),
            round: 0,
        });
        state.push(AgentContribution {
            agent: agent("p", "Pragmatist", 0.3),
            content: "Very long argument about microservices...".into(),
            core_claim: Some("Use what works.".into()),
            round: 0,
        });
        state.set_compacted(
            0,
            "[Skeptic]: Opposed the proposal.\n[Pragmatist]: Supported compromise.".into(),
        );
        state.advance_round();

        // Round 1 contributions
        state.push(AgentContribution {
            agent: agent("s", "Skeptic", 0.7),
            content: "Round 1 full text from Skeptic.".into(),
            core_claim: None,
            round: 1,
        });
        state.advance_round();

        // At round 2, round 0 should be compacted, round 1 should be full
        let a = agent("s", "Skeptic", 0.7);
        let (prompt, _) = build_agent_system_prompt(&a, "Topic", 2, 3, &state, None);

        // Round 0 should show compacted summary
        assert!(prompt.contains("=== Round 1 (compacted) ==="));
        assert!(prompt.contains("[Skeptic]: Opposed the proposal."));
        assert!(prompt.contains("[Pragmatist]: Supported compromise."));
        // Full text from round 0 should NOT appear
        assert!(!prompt.contains("Very long argument about monoliths"));

        // Round 1 should show full text
        assert!(prompt.contains("=== Round 2 ==="));
        assert!(prompt.contains("Round 1 full text from Skeptic."));
    }

    #[test]
    fn uncompacted_round_shows_full_text() {
        let mut state = CouncilState::new();
        state.push(AgentContribution {
            agent: agent("s", "Skeptic", 0.7),
            content: "Full argument text.".into(),
            core_claim: None,
            round: 0,
        });
        state.advance_round();

        // No compaction applied — should show full text
        let a = agent("p", "Pragmatist", 0.3);
        let (prompt, _) = build_agent_system_prompt(&a, "Topic", 1, 3, &state, None);

        assert!(prompt.contains("=== Round 1 ==="));
        assert!(!prompt.contains("(compacted)"));
        assert!(prompt.contains("Full argument text."));
    }

    // ── filesystem context tests ─────────────────────────────────────────

    #[test]
    fn cwd_none_omits_filesystem_context() {
        let state = CouncilState::new();
        let a = agent("s", "Skeptic", 0.7);
        let (prompt, _) = build_agent_system_prompt(&a, "Topic", 0, 1, &state, None);
        assert!(!prompt.contains("filesystem tools"));
        assert!(!prompt.contains("Working directory"));
    }

    #[test]
    fn cwd_some_injects_filesystem_context() {
        let state = CouncilState::new();
        let a = agent("s", "Skeptic", 0.7);
        let dir = std::path::PathBuf::from("/tmp/my-project");
        let (prompt, _) = build_agent_system_prompt(&a, "Topic", 0, 1, &state, Some(&dir));
        assert!(prompt.contains("filesystem tools (read_file, list_directory, grep_search)"));
        assert!(prompt.contains("Working directory: /tmp/my-project"));
    }
}
