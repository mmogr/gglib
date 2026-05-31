//! Post-round judge evaluation with adaptive early stopping.
//!
//! After each debate round, an optional neutral judge agent evaluates the
//! transcript, produces a narrative summary, and determines whether the
//! agents have reached consensus.  If consensus is detected and the
//! minimum-rounds threshold has been met, the debate runner skips remaining
//! rounds and proceeds directly to synthesis.
//!
//! # Robust marker parsing
//!
//! The judge's output must contain a `CONSENSUS_REACHED: true/false` line.
//! [`parse_judge_verdict`] uses case-insensitive, whitespace-tolerant
//! matching to handle common LLM quirks.

use std::collections::HashSet;
use std::sync::Arc;

use tokio::sync::mpsc;
use tracing::{debug, warn};

use gglib_core::domain::council::events::CouncilEvent;
use gglib_core::ports::{LlmCompletionPort, ToolExecutorPort};
use gglib_core::{AGENT_EVENT_CHANNEL_CAPACITY, AgentConfig, AgentEvent, AgentMessage};

use crate::AgentLoop;

use super::history::format_synthesis_transcript;
use super::prompts::JUDGE_PROMPT;
use super::state::DebateState;

/// The judge's parsed verdict after evaluating a round.
#[derive(Debug, Clone)]
pub(super) struct JudgeVerdict {
    /// The judge's narrative assessment of the debate state.
    pub assessment_text: String,
    /// Whether the judge determined consensus has been reached.
    pub consensus_reached: bool,
}

/// Run the judge evaluation for the given round.
///
/// Emits `DebateJudgeStarted`, streams `DebateJudgeTextDelta` tokens, then
/// emits `DebateJudgeSummary` with the parsed verdict.
///
/// Returns `None` if the channel is closed or the judge produces no output
/// (the caller treats this as "no consensus").
#[allow(clippy::too_many_arguments)]
pub(super) async fn run_judge(
    node_id: &str,
    round: u32,
    total_rounds: u32,
    min_rounds_before_stop: u32,
    topic: &str,
    llm: &Arc<dyn LlmCompletionPort>,
    tool_executor: &Arc<dyn ToolExecutorPort>,
    state: &DebateState,
    council_tx: &mpsc::Sender<CouncilEvent>,
) -> Option<JudgeVerdict> {
    // Announce the judge phase.
    if council_tx
        .send(CouncilEvent::DebateJudgeStarted {
            node_id: node_id.to_owned(),
            round: round + 1, // 1-based
        })
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

    // Bridge agent events → judge events.
    let mut content: Option<String> = None;
    while let Some(event) = agent_rx.recv().await {
        match event {
            AgentEvent::TextDelta { content: delta } => {
                let _ = council_tx
                    .send(CouncilEvent::DebateJudgeTextDelta {
                        node_id: node_id.to_owned(),
                        delta,
                    })
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
        warn!(round, "debate judge produced no output");
        return None;
    }

    let verdict = parse_judge_verdict(&raw, round);
    let early_stop_recommended = verdict.consensus_reached && round >= min_rounds_before_stop;

    let _ = council_tx
        .send(CouncilEvent::DebateJudgeSummary {
            node_id: node_id.to_owned(),
            round: round + 1, // 1-based
            consensus_reached: verdict.consensus_reached,
            early_stop_recommended,
            assessment_text: verdict.assessment_text.clone(),
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
/// The summary is everything *before* the marker line.
fn parse_judge_verdict(raw: &str, round: u32) -> JudgeVerdict {
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
        debug!(
            round,
            "judge output missing CONSENSUS_REACHED marker, defaulting to false"
        );
    }

    let assessment_text = marker_line_idx.map_or_else(
        || raw.trim().to_owned(),
        |idx| lines[..idx].to_vec().join("\n").trim().to_owned(),
    );

    JudgeVerdict {
        assessment_text,
        consensus_reached,
    }
}

/// Try to extract a boolean value from a line containing `CONSENSUS_REACHED`.
fn extract_consensus_value(line: &str) -> Option<bool> {
    // Strip common markdown wrappers.
    let cleaned: String = line.chars().filter(|c| *c != '*' && *c != '`').collect();
    let lower = cleaned.to_lowercase();

    let idx = lower.find("consensus_reached")?;
    let after = &lower[idx + "consensus_reached".len()..];

    // Find the colon.
    let colon_idx = after.find(':')?;
    let value_part = after[colon_idx + 1..].trim();

    if value_part.starts_with("true") {
        Some(true)
    } else if value_part.starts_with("false") {
        Some(false)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_consensus_true() {
        let raw = "The agents have converged.\nCONSENSUS_REACHED: true";
        let verdict = parse_judge_verdict(raw, 0);
        assert!(verdict.consensus_reached);
        assert_eq!(verdict.assessment_text, "The agents have converged.");
    }

    #[test]
    fn parse_consensus_false() {
        let raw = "Disagreement remains.\nCONSENSUS_REACHED: false";
        let verdict = parse_judge_verdict(raw, 0);
        assert!(!verdict.consensus_reached);
    }

    #[test]
    fn parse_consensus_markdown_bold() {
        let raw = "Summary.\n**CONSENSUS_REACHED:** true";
        let verdict = parse_judge_verdict(raw, 0);
        assert!(verdict.consensus_reached);
    }

    #[test]
    fn parse_consensus_uppercase_value() {
        let raw = "Summary.\nCONSENSUS_REACHED: TRUE";
        let verdict = parse_judge_verdict(raw, 0);
        assert!(verdict.consensus_reached);
    }

    #[test]
    fn parse_missing_marker_defaults_false() {
        let raw = "No marker in this output.";
        let verdict = parse_judge_verdict(raw, 0);
        assert!(!verdict.consensus_reached);
        // Full text should be used as assessment.
        assert_eq!(verdict.assessment_text, "No marker in this output.");
    }
}
