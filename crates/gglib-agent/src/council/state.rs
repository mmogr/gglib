//! Round and contribution state for a running council deliberation.
//!
//! [`CouncilState`] accumulates agent contributions as they complete,
//! organised by round.  It is the single mutable data structure that the
//! orchestrator writes to and that [`crate::council::history`] reads from
//! when assembling per-agent context.
//!
//! # Round compaction
//!
//! After a round completes, the orchestrator may store a compacted summary
//! via [`CouncilState::set_compacted`].  When compacted text exists for a
//! round, the history module uses the short summary instead of the full
//! agent contributions, keeping the context window manageable in long
//! debates.

use std::collections::HashMap;

use super::config::CouncilAgent;

// ─── contribution ────────────────────────────────────────────────────────────

/// A completed agent contribution within a single round.
#[derive(Debug, Clone)]
pub struct AgentContribution {
    /// Agent metadata snapshot (id, name, perspective, etc.).
    pub agent: CouncilAgent,
    /// The full text produced by the agent during this turn.
    pub content: String,
    /// Extracted core claim, or `None` if the model omitted the marker.
    pub core_claim: Option<String>,
    /// Zero-indexed round number.
    pub round: u32,
}

// ─── core claim extraction ───────────────────────────────────────────────────

/// Marker prefix that agents are instructed to include in their response.
const CORE_CLAIM_PREFIX: &str = "CORE CLAIM:";

/// Fault-tolerant extraction of a `CORE CLAIM: ...` line from a response.
///
/// Scans from the **end** of the text (the prompt asks agents to place the
/// claim at the bottom).  Returns `None` without error if the marker is
/// absent — this is expected behaviour for small models that may ignore
/// formatting instructions.
#[must_use]
pub fn extract_core_claim(content: &str) -> Option<String> {
    content
        .lines()
        .rev()
        .find_map(|line| {
            let trimmed = line.trim();
            trimmed
                .strip_prefix(CORE_CLAIM_PREFIX)
                .map(|rest| rest.trim().to_owned())
        })
        .filter(|s| !s.is_empty())
}

// ─── state ───────────────────────────────────────────────────────────────────

/// Accumulates contributions for an in-progress council run.
///
/// Written to by the orchestrator after each agent turn completes.
/// Read by the history module to assemble transcript context.
#[derive(Debug, Default)]
pub struct CouncilState {
    /// All contributions, in insertion order.
    contributions: Vec<AgentContribution>,
    /// Current round (zero-indexed).
    current_round: u32,
    /// Compacted round summaries, keyed by zero-indexed round number.
    ///
    /// When the history module encounters a compacted round, it uses this
    /// summary instead of replaying all individual contributions.
    compacted: HashMap<u32, String>,
}

impl CouncilState {
    /// Create a new, empty state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a completed agent turn.
    pub fn push(&mut self, contribution: AgentContribution) {
        self.contributions.push(contribution);
    }

    /// Advance to the next round.
    pub const fn advance_round(&mut self) {
        self.current_round += 1;
    }

    /// Current round number (zero-indexed).
    #[must_use]
    pub const fn current_round(&self) -> u32 {
        self.current_round
    }

    /// All contributions across all rounds, in insertion order.
    #[must_use]
    pub fn all_contributions(&self) -> &[AgentContribution] {
        &self.contributions
    }

    /// Contributions for a specific round.
    #[must_use]
    pub fn contributions_for_round(&self, round: u32) -> Vec<&AgentContribution> {
        self.contributions
            .iter()
            .filter(|c| c.round == round)
            .collect()
    }

    /// All completed rounds (each round is a `Vec` of contributions).
    ///
    /// Rounds are returned in order `0..=current_round` but only those
    /// that actually have at least one contribution.
    #[must_use]
    pub fn rounds_with_contributions(&self) -> Vec<(u32, Vec<&AgentContribution>)> {
        let max = self
            .contributions
            .iter()
            .map(|c| c.round)
            .max()
            .unwrap_or(0);
        (0..=max)
            .map(|r| (r, self.contributions_for_round(r)))
            .filter(|(_, cs)| !cs.is_empty())
            .collect()
    }

    /// Store a compacted summary for a completed round.
    ///
    /// The history module will use this instead of the full contributions
    /// when building agent context for subsequent rounds.
    pub fn set_compacted(&mut self, round: u32, summary: String) {
        self.compacted.insert(round, summary);
    }

    /// Retrieve the compacted summary for a round, if any.
    #[must_use]
    pub fn compacted_summary(&self, round: u32) -> Option<&str> {
        self.compacted.get(&round).map(String::as_str)
    }

    /// Whether a given round has been compacted.
    #[must_use]
    pub fn is_compacted(&self, round: u32) -> bool {
        self.compacted.contains_key(&round)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::council::config::CouncilAgent;

    fn test_agent(id: &str) -> CouncilAgent {
        CouncilAgent {
            id: id.into(),
            name: id.into(),
            color: "#000".into(),
            persona: "p".into(),
            perspective: "v".into(),
            contentiousness: 0.5,
            tool_filter: None,
        }
    }

    // ── extract_core_claim ───────────────────────────────────────────────

    #[test]
    fn extract_claim_at_end() {
        let text = "Some argument.\nCORE CLAIM: Monoliths scale better.";
        assert_eq!(
            extract_core_claim(text).as_deref(),
            Some("Monoliths scale better.")
        );
    }

    #[test]
    fn extract_claim_with_surrounding_whitespace() {
        let text = "Argument.\n  CORE CLAIM:   Spaced claim.  \n";
        assert_eq!(extract_core_claim(text).as_deref(), Some("Spaced claim."));
    }

    #[test]
    fn missing_claim_returns_none() {
        let text = "Just a regular response with no marker.";
        assert!(extract_core_claim(text).is_none());
    }

    #[test]
    fn empty_claim_returns_none() {
        let text = "Text.\nCORE CLAIM:   \n";
        assert!(extract_core_claim(text).is_none());
    }

    #[test]
    fn claim_in_middle_still_found() {
        let text = "Intro.\nCORE CLAIM: Middle claim.\nPost-text.";
        assert_eq!(extract_core_claim(text).as_deref(), Some("Middle claim."));
    }

    // ── CouncilState ─────────────────────────────────────────────────────

    #[test]
    fn empty_state() {
        let state = CouncilState::new();
        assert_eq!(state.current_round(), 0);
        assert!(state.all_contributions().is_empty());
        assert!(state.rounds_with_contributions().is_empty());
    }

    #[test]
    fn push_and_round_tracking() {
        let mut state = CouncilState::new();
        state.push(AgentContribution {
            agent: test_agent("a"),
            content: "Round 0 from a".into(),
            core_claim: None,
            round: 0,
        });
        state.push(AgentContribution {
            agent: test_agent("b"),
            content: "Round 0 from b".into(),
            core_claim: Some("Claim B.".into()),
            round: 0,
        });
        assert_eq!(state.all_contributions().len(), 2);
        assert_eq!(state.contributions_for_round(0).len(), 2);
        assert_eq!(state.contributions_for_round(1).len(), 0);

        state.advance_round();
        assert_eq!(state.current_round(), 1);

        state.push(AgentContribution {
            agent: test_agent("a"),
            content: "Round 1 from a".into(),
            core_claim: None,
            round: 1,
        });
        assert_eq!(state.contributions_for_round(1).len(), 1);

        let rounds = state.rounds_with_contributions();
        assert_eq!(rounds.len(), 2);
        assert_eq!(rounds[0].0, 0);
        assert_eq!(rounds[0].1.len(), 2);
        assert_eq!(rounds[1].0, 1);
        assert_eq!(rounds[1].1.len(), 1);
    }

    // ── compaction ───────────────────────────────────────────────────────

    #[test]
    fn compacted_summary_round_trip() {
        let mut state = CouncilState::new();
        assert!(!state.is_compacted(0));
        assert!(state.compacted_summary(0).is_none());

        state.set_compacted(0, "Round 0 summary.".into());
        assert!(state.is_compacted(0));
        assert_eq!(state.compacted_summary(0), Some("Round 0 summary."));
        assert!(!state.is_compacted(1));
    }

    #[test]
    fn compacted_overwrites_previous() {
        let mut state = CouncilState::new();
        state.set_compacted(0, "First.".into());
        state.set_compacted(0, "Second.".into());
        assert_eq!(state.compacted_summary(0), Some("Second."));
    }
}
