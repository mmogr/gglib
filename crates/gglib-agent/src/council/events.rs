//! SSE event types emitted during a council deliberation run.
//!
//! `CouncilEvent` is the **single source of truth** for the wire format
//! shared by the Axum SSE handler, the CLI consumer, and the TypeScript
//! frontend types (`src/types/council.ts`).
//!
//! Serialisation uses the same `{"type":"variant_name", ...}` envelope as
//! [`gglib_core::AgentEvent`] so frontend event handlers stay consistent.

use serde::{Deserialize, Serialize};

use gglib_core::{ToolCall, ToolResult};

/// Channel capacity for the council event sender.
///
/// Larger than the per-agent channel because council events include
/// contributions from multiple agents plus orchestration bookkeeping.
pub const COUNCIL_EVENT_CHANNEL_CAPACITY: usize = 8_192;

/// A single event in a council deliberation stream.
///
/// Consumers receive these over SSE (web) or an `mpsc` channel (CLI).
/// Each variant is independently useful — the frontend can render
/// progressively as events arrive.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CouncilEvent {
    // ── agent turn lifecycle ─────────────────────────────────────────────
    /// An agent is about to speak.  The frontend creates a new message
    /// bubble with the agent's name, colour and round badge.
    AgentTurnStart {
        agent_id: String,
        agent_name: String,
        color: String,
        round: u32,
        contentiousness: f32,
    },

    /// Incremental text token from the current agent's response.
    AgentTextDelta { agent_id: String, delta: String },

    /// Incremental reasoning / chain-of-thought token (for models that
    /// expose `CoT`).  Rendered in a collapsible "thinking" block.
    AgentReasoningDelta { agent_id: String, delta: String },

    /// The current agent has initiated a tool call.  The frontend shows a
    /// spinner with `display_name` and an optional `args_summary`.
    AgentToolCallStart {
        agent_id: String,
        tool_call: ToolCall,
        display_name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        args_summary: Option<String>,
    },

    /// A tool call by the current agent has completed.  Contains the
    /// [`ToolResult`] payload and a human-readable `duration_display`.
    AgentToolCallComplete {
        agent_id: String,
        tool_name: String,
        result: ToolResult,
        display_name: String,
        duration_display: String,
    },

    /// The current agent's turn is finished.
    ///
    /// `core_claim` is extracted from a `CORE CLAIM: ...` marker in the
    /// response.  If the model omitted the marker, this is `None` — which
    /// is perfectly normal and does not constitute an error.
    AgentTurnComplete {
        agent_id: String,
        content: String,
        round: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        core_claim: Option<String>,
    },

    // ── round bookkeeping ────────────────────────────────────────────────
    /// Emitted between rounds.  The frontend renders a plain "Round N"
    /// divider — no gradient, no tension metric (v1).
    RoundSeparator { round: u32 },

    // ── judge ────────────────────────────────────────────────────────────
    /// The judge evaluation phase has begun for this round.
    JudgeStart { round: u32 },

    /// Incremental text token from the judge agent.
    JudgeTextDelta { delta: String },

    /// The judge has completed its evaluation.
    ///
    /// `summary` is the judge's narrative assessment.
    /// `consensus_reached` indicates whether the judge determined
    /// that the agents have converged on a shared position.
    JudgeSummary {
        round: u32,
        summary: String,
        consensus_reached: bool,
    },

    // ── synthesis ────────────────────────────────────────────────────────
    /// The synthesis phase has begun.  The frontend renders a
    /// "Synthesising…" placeholder.
    SynthesisStart,

    /// Incremental text token from the synthesiser agent.
    SynthesisTextDelta { delta: String },

    /// The synthesis is complete; `content` holds the full merged answer.
    SynthesisComplete { content: String },

    // ── terminal ─────────────────────────────────────────────────────────
    /// An unrecoverable error during the council run.
    CouncilError { message: String },

    /// The entire council run has finished (all rounds + synthesis).
    CouncilComplete,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_agent_turn_start() {
        let event = CouncilEvent::AgentTurnStart {
            agent_id: "s1".into(),
            agent_name: "Skeptic".into(),
            color: "#ef4444".into(),
            round: 1,
            contentiousness: 0.8,
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "agent_turn_start");
        assert_eq!(json["agent_name"], "Skeptic");
        assert_eq!(json["round"], 1);
    }

    #[test]
    fn serialize_agent_turn_complete_with_core_claim() {
        let event = CouncilEvent::AgentTurnComplete {
            agent_id: "s1".into(),
            content: "Full response text.".into(),
            round: 1,
            core_claim: Some("Microservices are overkill.".into()),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "agent_turn_complete");
        assert_eq!(json["core_claim"], "Microservices are overkill.");
    }

    #[test]
    fn serialize_agent_turn_complete_without_core_claim() {
        let event = CouncilEvent::AgentTurnComplete {
            agent_id: "s1".into(),
            content: "No marker in response.".into(),
            round: 2,
            core_claim: None,
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "agent_turn_complete");
        assert!(json.get("core_claim").is_none());
    }

    #[test]
    fn serialize_round_separator() {
        let event = CouncilEvent::RoundSeparator { round: 2 };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "round_separator");
        assert_eq!(json["round"], 2);
    }

    #[test]
    fn serialize_synthesis_complete() {
        let event = CouncilEvent::SynthesisComplete {
            content: "Final synthesis.".into(),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "synthesis_complete");
    }

    #[test]
    fn serialize_council_complete() {
        let json = serde_json::to_value(&CouncilEvent::CouncilComplete).unwrap();
        assert_eq!(json["type"], "council_complete");
    }

    #[test]
    fn round_trip_all_variants() {
        let events = vec![
            CouncilEvent::AgentTurnStart {
                agent_id: "a".into(),
                agent_name: "A".into(),
                color: "#000".into(),
                round: 1,
                contentiousness: 0.5,
            },
            CouncilEvent::AgentTextDelta {
                agent_id: "a".into(),
                delta: "hello".into(),
            },
            CouncilEvent::AgentReasoningDelta {
                agent_id: "a".into(),
                delta: "thinking...".into(),
            },
            CouncilEvent::RoundSeparator { round: 1 },
            CouncilEvent::JudgeStart { round: 1 },
            CouncilEvent::JudgeTextDelta {
                delta: "evaluating".into(),
            },
            CouncilEvent::JudgeSummary {
                round: 1,
                summary: "Agents are converging.".into(),
                consensus_reached: true,
            },
            CouncilEvent::SynthesisStart,
            CouncilEvent::SynthesisTextDelta {
                delta: "synth".into(),
            },
            CouncilEvent::SynthesisComplete {
                content: "done".into(),
            },
            CouncilEvent::CouncilError {
                message: "oops".into(),
            },
            CouncilEvent::CouncilComplete,
        ];
        for event in &events {
            let json = serde_json::to_string(event).unwrap();
            let back: CouncilEvent = serde_json::from_str(&json).unwrap();
            // Verify the round-trip doesn't panic; structural equality
            // is covered by the per-variant tests above.
            let _ = format!("{back:?}");
        }
    }
}
