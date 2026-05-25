//! Round compaction — LLM-driven summarisation of completed debate rounds.
//!
//! After a round completes (and before the next round begins), the runner
//! may compact the round to keep prompt sizes manageable in long debates.
//!
//! # Robust parsing
//!
//! The compactor's output must contain `SUMMARY(Agent Name): ...` lines.
//! [`parse_compacted_summaries`] uses case-insensitive, whitespace-tolerant
//! matching to handle common LLM quirks.
//!
//! If parsing fails to extract any summaries, the round is left
//! uncompacted — agents simply see the full transcript (graceful
//! degradation).

use std::collections::HashSet;
use std::fmt::Write;
use std::sync::Arc;

use tokio::sync::mpsc;
use tracing::{debug, warn};

use gglib_core::domain::council::events::CouncilEvent;
use gglib_core::ports::{LlmCompletionPort, ToolExecutorPort};
use gglib_core::{AGENT_EVENT_CHANNEL_CAPACITY, AgentConfig, AgentEvent, AgentMessage};

use crate::AgentLoop;

use super::prompts::COMPACTION_PROMPT;
use super::state::DebateState;

/// Run the compaction pass for a completed debate round.
///
/// Sends the round's transcript to a single-iteration `AgentLoop` (no tools)
/// and parses the output into per-agent summaries.  If successful, stores
/// the compacted text in `state` and emits a `DebateRoundCompacted` event.
pub(super) async fn compact_round(
    node_id: &str,
    round: u32,
    state: &mut DebateState,
    llm: &Arc<dyn LlmCompletionPort>,
    tool_executor: &Arc<dyn ToolExecutorPort>,
    council_tx: &mpsc::Sender<CouncilEvent>,
) {
    let transcript = format_round_transcript(state, round);
    if transcript.is_empty() {
        return;
    }

    #[allow(clippy::literal_string_with_formatting_args)]
    let system = COMPACTION_PROMPT
        .replace("{round}", &(round + 1).to_string())
        .replace("{transcript}", &transcript);

    let messages = vec![
        AgentMessage::System { content: system },
        AgentMessage::User {
            content: format!("Summarise round {} of the debate.", round + 1),
        },
    ];

    // Compactor gets no tools — pure text generation.
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

    // Collect the output (we don't stream compaction tokens to the client).
    let mut content: Option<String> = None;
    while let Some(event) = agent_rx.recv().await {
        if let AgentEvent::FinalAnswer { content: answer } = event {
            content = Some(answer);
        }
    }

    let _ = handle.await;

    let raw = content.unwrap_or_default();
    if raw.is_empty() {
        warn!(round, "debate compaction agent produced no output");
        return;
    }

    // Collect agent names from this round for validation.
    let round_agents: Vec<String> = state
        .contributions_for_round(round)
        .iter()
        .map(|c| c.agent.name.clone())
        .collect();

    let summaries = parse_compacted_summaries(&raw, &round_agents);
    if summaries.is_empty() {
        warn!(
            round,
            "debate compaction produced no parseable summaries, keeping full transcript"
        );
        return;
    }

    debug!(
        round,
        summary_count = summaries.len(),
        "debate round compacted successfully"
    );

    // Build the compacted text block.
    let mut compacted = String::new();
    for (name, summary) in &summaries {
        let _ = writeln!(compacted, "[{name}]: {summary}");
    }

    state.set_compacted(round, compacted.trim().to_owned());

    // Emit event.
    let _ = council_tx
        .send(CouncilEvent::DebateRoundCompacted {
            node_id: node_id.to_owned(),
            round: round + 1, // 1-based
            summary: state
                .compacted_summary(round)
                .unwrap_or_default()
                .to_owned(),
        })
        .await;
}

/// Format a single round's contributions for the compaction prompt.
fn format_round_transcript(state: &DebateState, round: u32) -> String {
    let mut out = String::new();
    for c in state.contributions_for_round(round) {
        let _ = writeln!(out, "[{}]: {}", c.agent.name, c.content);
    }
    out
}

/// Robust extraction of `SUMMARY(Agent Name): ...` lines from LLM output.
///
/// Returns `(agent_name, summary_text)` pairs.  If no summaries are found,
/// returns an empty vec (the caller should keep the full transcript).
pub(super) fn parse_compacted_summaries(
    raw: &str,
    expected_agents: &[String],
) -> Vec<(String, String)> {
    let mut results = Vec::new();

    for line in raw.lines() {
        if let Some((name, summary)) = extract_summary_line(line) {
            results.push((name, summary));
        }
    }

    if !results.is_empty() {
        return results;
    }

    // Fallback: try to match lines that look like "[Agent]: summary"
    for line in raw.lines() {
        if let Some((name, summary)) = extract_bracket_line(line, expected_agents) {
            results.push((name, summary));
        }
    }

    results
}

/// Try to extract a `SUMMARY(name): text` pattern from a single line.
///
/// Strips markdown bold (`**`) and backtick (`` ` ``) wrapping before
/// matching.
fn extract_summary_line(line: &str) -> Option<(String, String)> {
    let cleaned: String = line.chars().filter(|c| *c != '*' && *c != '`').collect();
    let lower = cleaned.to_lowercase();

    let summary_idx = lower.find("summary")?;
    let after_keyword = &cleaned[summary_idx + "summary".len()..];

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
    let summary = after_close[colon_idx + 1..].trim().to_owned();

    if summary.is_empty() {
        return None;
    }

    Some((name, summary))
}

/// Try to extract an `[Agent Name]: summary` pattern from a single line.
fn extract_bracket_line(line: &str, expected_agents: &[String]) -> Option<(String, String)> {
    let trimmed = line.trim();
    if !trimmed.starts_with('[') {
        return None;
    }
    let close = trimmed.find(']')?;
    let name = trimmed[1..close].trim().to_owned();

    // Only accept if the name matches one of the expected agents (case-insensitive).
    let matches = expected_agents
        .iter()
        .any(|a| a.to_lowercase() == name.to_lowercase());
    if !matches {
        return None;
    }

    let after = &trimmed[close + 1..];
    let colon_idx = after.find(':')?;
    let summary = after[colon_idx + 1..].trim().to_owned();

    if summary.is_empty() {
        return None;
    }

    Some((name, summary))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_canonical_summary_lines() {
        let raw = "SUMMARY(Alice): She argued X.\nSUMMARY(Bob): He argued Y.";
        let agents = vec!["Alice".into(), "Bob".into()];
        let results = parse_compacted_summaries(raw, &agents);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, "Alice");
        assert_eq!(results[0].1, "She argued X.");
        assert_eq!(results[1].0, "Bob");
        assert_eq!(results[1].1, "He argued Y.");
    }

    #[test]
    fn parse_markdown_wrapped_summary() {
        let raw = "**SUMMARY(Skeptic):** They argued against.";
        let agents = vec!["Skeptic".into()];
        let results = parse_compacted_summaries(raw, &agents);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "Skeptic");
    }

    #[test]
    fn parse_lowercase_summary() {
        let raw = "summary(skeptic): lowercase summary here.";
        let agents = vec!["skeptic".into()];
        let results = parse_compacted_summaries(raw, &agents);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn fallback_bracket_format() {
        let raw = "[Alice]: fallback summary.\n[Bob]: another fallback.";
        let agents = vec!["Alice".into(), "Bob".into()];
        let results = parse_compacted_summaries(raw, &agents);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn unknown_agent_bracket_skipped() {
        let raw = "[Attacker]: trying to inject.";
        let agents = vec!["Alice".into()];
        let results = parse_compacted_summaries(raw, &agents);
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn no_summaries_returns_empty() {
        let raw = "No parseable summaries here.";
        let agents = vec!["Alice".into()];
        let results = parse_compacted_summaries(raw, &agents);
        assert!(results.is_empty());
    }
}
