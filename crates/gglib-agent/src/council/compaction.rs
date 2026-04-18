//! Round compaction — LLM-driven summarisation of completed debate rounds.
//!
//! After a round completes (and before the next round begins), the
//! orchestrator may run the compactor to produce a short per-agent summary
//! of the round's contributions.  The compacted text replaces the full
//! transcript in subsequent agents' context windows, keeping prompt sizes
//! manageable in long debates (4+ rounds).
//!
//! # Robust parsing
//!
//! The compactor's output must contain `SUMMARY(Agent Name): ...` lines.
//! [`parse_compacted_summaries`] uses case-insensitive, whitespace-tolerant
//! matching to handle common LLM quirks:
//! - Markdown bold/backtick wrapping (`**SUMMARY(Skeptic):** ...`)
//! - Extra spacing around the colon
//! - Varying casing (`summary(name)`, `Summary(Name)`)
//! - Conversational filler before/after the markers
//!
//! If parsing fails to extract any summaries, the round is left
//! uncompacted — agents simply see the full transcript (graceful
//! degradation).

use std::collections::HashSet;
use std::fmt::Write;
use std::sync::Arc;

use tokio::sync::mpsc;
use tracing::{debug, warn};

use gglib_core::{
    AGENT_EVENT_CHANNEL_CAPACITY, AgentConfig, AgentEvent, AgentMessage, LlmCompletionPort,
    ToolExecutorPort,
};

use crate::AgentLoop;

use super::events::CouncilEvent;
use super::prompts::COMPACTION_PROMPT;
use super::state::CouncilState;

/// Run the compaction pass for a completed round.
///
/// Sends the round's transcript to a single-iteration `AgentLoop` (no
/// tools) and parses the output into per-agent summaries.  If successful,
/// stores the compacted text in `state`.
///
/// This function is fire-and-forget from the orchestrator's perspective —
/// if the LLM produces unparseable output or the channel is closed, the
/// round simply stays uncompacted and agents see the full transcript.
pub(super) async fn compact_round(
    round: u32,
    state: &mut CouncilState,
    llm: &Arc<dyn LlmCompletionPort>,
    tool_executor: &Arc<dyn ToolExecutorPort>,
    council_tx: &mpsc::Sender<CouncilEvent>,
    _topic: &str,
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
        warn!(round, "compaction agent produced no output");
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
            "compaction produced no parseable summaries, keeping full transcript"
        );
        return;
    }

    debug!(
        round,
        summary_count = summaries.len(),
        "round compacted successfully"
    );

    // Build the compacted text block.
    let mut compacted = String::new();
    for (name, summary) in &summaries {
        let _ = writeln!(compacted, "[{name}]: {summary}");
    }

    state.set_compacted(round, compacted.trim().to_owned());

    // Emit a council event so the frontend/CLI can optionally display it.
    let _ = council_tx
        .send(CouncilEvent::RoundCompacted {
            round,
            summary: state
                .compacted_summary(round)
                .unwrap_or_default()
                .to_owned(),
        })
        .await;
}

/// Format a single round's contributions for the compaction prompt.
fn format_round_transcript(state: &CouncilState, round: u32) -> String {
    let mut out = String::new();
    for c in state.contributions_for_round(round) {
        let _ = writeln!(out, "[{}]: {}", c.agent.name, c.content);
    }
    out
}

/// Robust extraction of `SUMMARY(Agent Name): ...` lines from LLM output.
///
/// Handles common LLM formatting quirks:
/// - `SUMMARY(Skeptic): They argued ...` (canonical)
/// - `**SUMMARY(Skeptic):** They argued ...` (markdown bold)
/// - `` `SUMMARY(Skeptic)`: They argued ... `` (backtick wrapping)
/// - `summary(skeptic): They argued ...` (lowercase)
/// - `SUMMARY (Skeptic) : They argued ...` (extra spaces)
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

    // If we found at least one summary, return what we have.
    // We don't require all agents — some may have been omitted by the LLM.
    if !results.is_empty() {
        return results;
    }

    // Fallback: try to match lines that look like "[Agent]: summary"
    // (some models may omit the SUMMARY() wrapper entirely).
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
/// parsing.
fn extract_summary_line(line: &str) -> Option<(String, String)> {
    // Strip markdown wrappers.
    let cleaned: String = line.chars().filter(|c| *c != '*' && *c != '`').collect();

    let lower = cleaned.to_lowercase();

    // Find "summary" followed by "(" somewhere.
    let summary_idx = lower.find("summary")?;
    let after_summary = &cleaned[summary_idx + "summary".len()..];

    // Skip optional whitespace, then expect '('.
    let after_ws = after_summary.trim_start();
    let after_paren = after_ws.strip_prefix('(')?;

    // Find the closing ')'.
    let close_idx = after_paren.find(')')?;
    let name = after_paren[..close_idx].trim();

    if name.is_empty() {
        return None;
    }

    // Everything after ')' — skip optional ':' and whitespace.
    let rest = &after_paren[close_idx + 1..];
    let rest = rest.trim_start();
    let rest = rest.strip_prefix(':').unwrap_or(rest);
    let rest = rest.trim();

    if rest.is_empty() {
        return None;
    }

    Some((name.to_owned(), rest.to_owned()))
}

/// Fallback: try to match `[Agent Name]: summary` lines against known agents.
fn extract_bracket_line(line: &str, expected_agents: &[String]) -> Option<(String, String)> {
    let trimmed = line.trim();
    let after_bracket = trimmed.strip_prefix('[')?;
    let close_idx = after_bracket.find(']')?;
    let name = after_bracket[..close_idx].trim();

    // Only accept if the name matches a known agent.
    let matched = expected_agents.iter().any(|a| a.eq_ignore_ascii_case(name));
    if !matched {
        return None;
    }

    let rest = &after_bracket[close_idx + 1..];
    let rest = rest.trim_start();
    let rest = rest.strip_prefix(':').unwrap_or(rest);
    let rest = rest.trim();

    if rest.is_empty() {
        return None;
    }

    Some((name.to_owned(), rest.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── extract_summary_line ─────────────────────────────────────────────

    #[test]
    fn canonical_summary_line() {
        let (name, text) =
            extract_summary_line("SUMMARY(Skeptic): They argued against the proposal.").unwrap();
        assert_eq!(name, "Skeptic");
        assert_eq!(text, "They argued against the proposal.");
    }

    #[test]
    fn markdown_bold_wrapping() {
        let (name, text) =
            extract_summary_line("**SUMMARY(Pragmatist):** A practical approach was suggested.")
                .unwrap();
        assert_eq!(name, "Pragmatist");
        assert_eq!(text, "A practical approach was suggested.");
    }

    #[test]
    fn backtick_wrapping() {
        let (name, text) =
            extract_summary_line("`SUMMARY(Expert)`: Domain-specific evidence was cited.").unwrap();
        assert_eq!(name, "Expert");
        assert_eq!(text, "Domain-specific evidence was cited.");
    }

    #[test]
    fn lowercase_summary() {
        let (name, text) =
            extract_summary_line("summary(devil's advocate): Everything is wrong.").unwrap();
        assert_eq!(name, "devil's advocate");
        assert_eq!(text, "Everything is wrong.");
    }

    #[test]
    fn extra_spacing() {
        let (name, text) =
            extract_summary_line("SUMMARY ( Skeptic ) :  They had concerns.  ").unwrap();
        assert_eq!(name, "Skeptic");
        assert_eq!(text, "They had concerns.");
    }

    #[test]
    fn empty_name_returns_none() {
        assert!(extract_summary_line("SUMMARY(): No name.").is_none());
    }

    #[test]
    fn empty_text_returns_none() {
        assert!(extract_summary_line("SUMMARY(Skeptic):").is_none());
        assert!(extract_summary_line("SUMMARY(Skeptic):   ").is_none());
    }

    #[test]
    fn no_summary_marker_returns_none() {
        assert!(extract_summary_line("Just some regular text.").is_none());
    }

    #[test]
    fn missing_close_paren_returns_none() {
        assert!(extract_summary_line("SUMMARY(Skeptic: missing paren").is_none());
    }

    // ── extract_bracket_line ─────────────────────────────────────────────

    #[test]
    fn bracket_line_matches_known_agent() {
        let agents = vec!["Skeptic".into(), "Pragmatist".into()];
        let (name, text) = extract_bracket_line("[Skeptic]: They disagreed.", &agents).unwrap();
        assert_eq!(name, "Skeptic");
        assert_eq!(text, "They disagreed.");
    }

    #[test]
    fn bracket_line_case_insensitive_match() {
        let agents = vec!["Skeptic".into()];
        let (name, _) = extract_bracket_line("[skeptic]: Something.", &agents).unwrap();
        assert_eq!(name, "skeptic");
    }

    #[test]
    fn bracket_line_unknown_agent_returns_none() {
        let agents = vec!["Skeptic".into()];
        assert!(extract_bracket_line("[Unknown]: Something.", &agents).is_none());
    }

    #[test]
    fn bracket_line_empty_text_returns_none() {
        let agents = vec!["Skeptic".into()];
        assert!(extract_bracket_line("[Skeptic]:", &agents).is_none());
    }

    // ── parse_compacted_summaries ────────────────────────────────────────

    #[test]
    fn parse_canonical_summaries() {
        let raw = "\
SUMMARY(Skeptic): Bad idea overall.
SUMMARY(Pragmatist): Practical compromise needed.
SUMMARY(Expert): Evidence supports option B.";

        let agents = vec!["Skeptic".into(), "Pragmatist".into(), "Expert".into()];
        let result = parse_compacted_summaries(raw, &agents);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].0, "Skeptic");
        assert_eq!(result[1].0, "Pragmatist");
        assert_eq!(result[2].0, "Expert");
    }

    #[test]
    fn parse_with_preamble_and_filler() {
        let raw = "\
Here are the summaries for this round:

SUMMARY(Skeptic): They had strong objections.
SUMMARY(Pragmatist): They proposed a middle ground.

Hope this helps!";

        let agents = vec!["Skeptic".into(), "Pragmatist".into()];
        let result = parse_compacted_summaries(raw, &agents);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn parse_fallback_to_bracket_format() {
        let raw = "\
[Skeptic]: They disagreed strongly.
[Pragmatist]: They offered a compromise.";

        let agents = vec!["Skeptic".into(), "Pragmatist".into()];
        let result = parse_compacted_summaries(raw, &agents);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].0, "Skeptic");
    }

    #[test]
    fn parse_empty_output() {
        let result = parse_compacted_summaries("", &["Skeptic".into()]);
        assert!(result.is_empty());
    }

    #[test]
    fn parse_no_valid_lines() {
        let raw = "I couldn't really summarise the debate.";
        let result = parse_compacted_summaries(raw, &["Skeptic".into()]);
        assert!(result.is_empty());
    }

    #[test]
    fn parse_markdown_bold_summaries() {
        let raw = "\
**SUMMARY(Skeptic):** Strong opposition based on cost analysis.
**SUMMARY(Advocate):** Supported the proposal citing long-term ROI.";

        let agents = vec!["Skeptic".into(), "Advocate".into()];
        let result = parse_compacted_summaries(raw, &agents);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn parse_partial_summaries_still_accepted() {
        // LLM only summarised one of two agents — we accept what we get.
        let raw = "SUMMARY(Skeptic): They had concerns.";
        let agents = vec!["Skeptic".into(), "Pragmatist".into()];
        let result = parse_compacted_summaries(raw, &agents);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, "Skeptic");
    }
}
