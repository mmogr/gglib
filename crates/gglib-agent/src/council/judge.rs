//! Post-round judge evaluation with adaptive early stopping.
//!
//! After each debate round, an optional neutral judge agent evaluates the
//! transcript, produces a narrative summary, and determines whether the
//! agents have reached consensus.  If consensus is detected and the
//! minimum-rounds threshold has been met, the orchestrator skips remaining
//! rounds and proceeds directly to synthesis.
//!
//! # Robust marker parsing
//!
//! The judge's output must contain a `CONSENSUS_REACHED: true/false` line.
//! [`parse_judge_verdict`] uses case-insensitive, whitespace-tolerant
//! matching to handle common LLM quirks:
//! - Markdown bold/backtick wrapping (`**CONSENSUS_REACHED:** true`)
//! - Leading prose / conversational filler
//! - Extra spacing around the colon
//! - Varying casing (`consensus_reached`, `Consensus_Reached`, etc.)

use std::collections::HashSet;
use std::sync::Arc;

use tokio::sync::mpsc;
use tracing::{debug, warn};

use gglib_core::{
    AGENT_EVENT_CHANNEL_CAPACITY, AgentConfig, AgentEvent, AgentMessage, LlmCompletionPort,
    ToolExecutorPort,
};

use crate::AgentLoop;

use super::config::JudgeConfig;
use super::events::CouncilEvent;
use super::history::format_synthesis_transcript;
use super::prompts::JUDGE_PROMPT;
use super::state::CouncilState;

/// The judge's parsed verdict after evaluating a round.
#[derive(Debug, Clone)]
pub(super) struct JudgeVerdict {
    /// The judge's narrative summary of the debate state.
    pub summary: String,
    /// Whether the judge determined consensus has been reached.
    pub consensus_reached: bool,
}

/// Run the judge evaluation for the given round.
///
/// Emits `JudgeStart`, streams `JudgeTextDelta` tokens, then emits
/// `JudgeSummary` with the parsed verdict.
///
/// Returns `None` if the channel is closed or the judge produces no
/// output (the orchestrator should treat this as "no consensus").
#[allow(clippy::too_many_arguments)]
pub(super) async fn run_judge(
    round: u32,
    total_rounds: u32,
    _judge_config: &JudgeConfig,
    llm: &Arc<dyn LlmCompletionPort>,
    tool_executor: &Arc<dyn ToolExecutorPort>,
    state: &CouncilState,
    council_tx: &mpsc::Sender<CouncilEvent>,
    topic: &str,
) -> Option<JudgeVerdict> {
    // Announce the judge phase.
    if council_tx
        .send(CouncilEvent::JudgeStart { round })
        .await
        .is_err()
    {
        return None;
    }

    let transcript = format_synthesis_transcript(state);

    #[allow(clippy::literal_string_with_formatting_args)]
    let system = JUDGE_PROMPT
        .replace("{topic}", topic)
        .replace("{round}", &(round + 1).to_string())
        .replace("{total_rounds}", &total_rounds.to_string())
        .replace("{transcript}", &transcript);

    let messages = vec![
        AgentMessage::System { content: system },
        AgentMessage::User {
            content: format!("Evaluate the debate state after round {}.", round + 1),
        },
    ];

    // Judge gets no tools — pure evaluation.
    let agent = AgentLoop::build(Arc::clone(llm), Arc::clone(tool_executor), Some(HashSet::new()));
    let mut config = AgentConfig::default();
    config.max_iterations = 1;

    let (agent_tx, mut agent_rx) = mpsc::channel::<AgentEvent>(AGENT_EVENT_CHANNEL_CAPACITY);

    let handle = {
        let agent = Arc::clone(&agent);
        tokio::spawn(async move { agent.run(messages, config, agent_tx).await })
    };

    // Bridge agent events → judge events.
    let mut content: Option<String> = None;
    while let Some(event) = agent_rx.recv().await {
        match event {
            AgentEvent::TextDelta { content: delta } => {
                let _ = council_tx
                    .send(CouncilEvent::JudgeTextDelta { delta })
                    .await;
            }
            AgentEvent::FinalAnswer { content: answer } => {
                content = Some(answer);
            }
            _ => {}
        }
    }

    let _ = handle.await;

    let raw = content.unwrap_or_default();
    if raw.is_empty() {
        warn!(round, "judge produced no output");
        return None;
    }

    let verdict = parse_judge_verdict(&raw, round);

    let _ = council_tx
        .send(CouncilEvent::JudgeSummary {
            round,
            summary: verdict.summary.clone(),
            consensus_reached: verdict.consensus_reached,
        })
        .await;

    Some(verdict)
}

/// Robust, case-insensitive extraction of the `CONSENSUS_REACHED` marker.
///
/// Handles common LLM output variations:
/// - `CONSENSUS_REACHED: true` (canonical)
/// - `**CONSENSUS_REACHED:** false` (markdown bold)
/// - `` `CONSENSUS_REACHED`: true `` (markdown backtick)
/// - `consensus_reached : True` (extra spacing, mixed case)
/// - `Consensus_Reached: FALSE` (title case)
///
/// The summary is everything *before* the marker line.  If the marker is
/// absent, defaults to `consensus_reached = false`.
fn parse_judge_verdict(raw: &str, round: u32) -> JudgeVerdict {
    // Scan lines from the end — the prompt asks for the marker at the bottom.
    let mut consensus_reached = false;
    let mut marker_line_idx: Option<usize> = None;

    let lines: Vec<&str> = raw.lines().collect();
    for (i, line) in lines.iter().enumerate().rev() {
        if let Some(value) = extract_consensus_value(line) {
            consensus_reached = value;
            marker_line_idx = Some(i);
            break;
        }
    }

    if marker_line_idx.is_none() {
        debug!(round, "judge output missing CONSENSUS_REACHED marker, defaulting to false");
    }

    // Summary = everything before the marker line (or the full text if no marker).
    let summary = marker_line_idx.map_or_else(
        || raw.trim().to_owned(),
        |idx| lines[..idx].to_vec().join("\n").trim().to_owned(),
    );

    JudgeVerdict {
        summary,
        consensus_reached,
    }
}

/// Try to extract a boolean value from a line containing `CONSENSUS_REACHED`.
///
/// Strips markdown formatting (`**`, `` ` ``), normalises whitespace, and
/// performs case-insensitive matching.  Returns `None` if the line does not
/// contain the marker.
fn extract_consensus_value(line: &str) -> Option<bool> {
    // Strip common markdown wrappers.
    let cleaned: String = line
        .chars()
        .filter(|c| *c != '*' && *c != '`')
        .collect();

    let lower = cleaned.to_lowercase();

    // Look for "consensus_reached" anywhere in the line.
    let idx = lower.find("consensus_reached")?;

    // Everything after the marker keyword.
    let after = &lower[idx + "consensus_reached".len()..];

    // Strip optional colon and whitespace.
    let after = after.trim_start();
    let after = after.strip_prefix(':').unwrap_or(after);
    let after = after.trim();

    // Parse the boolean value.
    if after.starts_with("true") || after.starts_with("yes") {
        Some(true)
    } else if after.starts_with("false") || after.starts_with("no") {
        Some(false)
    } else {
        None
    }
}

/// Whether the judge should allow early stopping at this round.
///
/// Returns `true` if the completed round count meets the minimum threshold
/// configured in [`JudgeConfig`].
pub(super) const fn may_stop_early(judge_config: &JudgeConfig, completed_rounds: u32) -> bool {
    completed_rounds >= judge_config.min_rounds_before_stop
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_judge_verdict ──────────────────────────────────────────────

    #[test]
    fn canonical_true() {
        let raw = "The agents agree on the core approach.\nCONSENSUS_REACHED: true";
        let v = parse_judge_verdict(raw, 0);
        assert!(v.consensus_reached);
        assert_eq!(v.summary, "The agents agree on the core approach.");
    }

    #[test]
    fn canonical_false() {
        let raw = "Significant disagreement remains.\nCONSENSUS_REACHED: false";
        let v = parse_judge_verdict(raw, 0);
        assert!(!v.consensus_reached);
        assert_eq!(v.summary, "Significant disagreement remains.");
    }

    #[test]
    fn markdown_bold_wrapping() {
        let raw = "Summary here.\n**CONSENSUS_REACHED:** true";
        let v = parse_judge_verdict(raw, 0);
        assert!(v.consensus_reached);
        assert_eq!(v.summary, "Summary here.");
    }

    #[test]
    fn markdown_backtick_wrapping() {
        let raw = "Summary.\n`CONSENSUS_REACHED`: false";
        let v = parse_judge_verdict(raw, 0);
        assert!(!v.consensus_reached);
    }

    #[test]
    fn mixed_case() {
        let raw = "Debate ongoing.\nConsensus_Reached: True";
        let v = parse_judge_verdict(raw, 0);
        assert!(v.consensus_reached);
    }

    #[test]
    fn extra_spacing() {
        let raw = "Still debating.\nconsensus_reached :  false";
        let v = parse_judge_verdict(raw, 0);
        assert!(!v.consensus_reached);
    }

    #[test]
    fn conversational_filler_before_marker() {
        let raw = "Here is my verdict:\n\nThe agents remain divided on implementation.\n\nCONSENSUS_REACHED: false";
        let v = parse_judge_verdict(raw, 0);
        assert!(!v.consensus_reached);
        assert!(v.summary.contains("Here is my verdict:"));
        assert!(v.summary.contains("remain divided"));
    }

    #[test]
    fn missing_marker_defaults_to_false() {
        let raw = "The debate is interesting but I cannot decide.";
        let v = parse_judge_verdict(raw, 0);
        assert!(!v.consensus_reached);
        assert_eq!(v.summary, raw);
    }

    #[test]
    fn yes_no_alternatives() {
        let raw = "All agree.\nCONSENSUS_REACHED: yes";
        assert!(parse_judge_verdict(raw, 0).consensus_reached);

        let raw = "No agreement.\nCONSENSUS_REACHED: no";
        assert!(!parse_judge_verdict(raw, 0).consensus_reached);
    }

    #[test]
    fn marker_in_middle_of_text() {
        // Some models might put text after the marker — the parser should
        // still find the last occurrence scanning from the bottom.
        let raw = "Round 1 summary.\nCONSENSUS_REACHED: true\nExtra text here.";
        let v = parse_judge_verdict(raw, 0);
        // The scanner finds the marker line and uses text before it as summary.
        // Because we scan from the end, the last CONSENSUS_REACHED line wins.
        // In this case there's only one, but "Extra text here." is after it.
        assert!(v.consensus_reached);
    }

    #[test]
    fn multiline_summary_preserved() {
        let raw = "Point 1.\nPoint 2.\nPoint 3.\n\nCONSENSUS_REACHED: false";
        let v = parse_judge_verdict(raw, 0);
        assert!(!v.consensus_reached);
        assert!(v.summary.contains("Point 1."));
        assert!(v.summary.contains("Point 3."));
    }

    // ── may_stop_early ───────────────────────────────────────────────────

    #[test]
    fn stop_early_respects_minimum() {
        let cfg = JudgeConfig {
            min_rounds_before_stop: 2,
        };
        assert!(!may_stop_early(&cfg, 0));
        assert!(!may_stop_early(&cfg, 1));
        assert!(may_stop_early(&cfg, 2));
        assert!(may_stop_early(&cfg, 3));
    }

    #[test]
    fn stop_early_default_min() {
        let cfg = JudgeConfig {
            min_rounds_before_stop: 1,
        };
        assert!(!may_stop_early(&cfg, 0));
        assert!(may_stop_early(&cfg, 1));
    }

    // ── extract_consensus_value ──────────────────────────────────────────

    #[test]
    fn extract_plain() {
        assert_eq!(extract_consensus_value("CONSENSUS_REACHED: true"), Some(true));
        assert_eq!(extract_consensus_value("CONSENSUS_REACHED: false"), Some(false));
    }

    #[test]
    fn extract_with_markdown() {
        assert_eq!(extract_consensus_value("**CONSENSUS_REACHED:** true"), Some(true));
        assert_eq!(extract_consensus_value("`CONSENSUS_REACHED`: false"), Some(false));
    }

    #[test]
    fn extract_no_marker() {
        assert_eq!(extract_consensus_value("some random text"), None);
    }

    #[test]
    fn extract_bad_value() {
        assert_eq!(extract_consensus_value("CONSENSUS_REACHED: maybe"), None);
    }
}
