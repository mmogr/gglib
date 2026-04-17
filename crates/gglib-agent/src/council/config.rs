//! Configuration types for the Council of Agents feature.
//!
//! A council is a group of agents with distinct personas who debate a topic
//! across multiple rounds, then produce a synthesised answer.  These types
//! describe the static configuration of a council run — who the agents are,
//! how many rounds to run, and what the synthesis should prioritise.

use serde::{Deserialize, Serialize};

// ─── per-agent config ────────────────────────────────────────────────────────

/// Describes a single agent participating in a council debate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CouncilAgent {
    /// Unique opaque identifier (e.g. a short slug or UUID).
    /// Defaults to empty; call [`SuggestedCouncil::backfill_defaults`]
    /// after deserialising LLM output to populate missing ids.
    #[serde(default)]
    pub id: String,

    /// Human-readable role title (e.g. "Devil's Advocate", "Pragmatist").
    pub name: String,

    /// CSS-compatible colour string for the agent's avatar and lane tint
    /// (e.g. `"#ef4444"` or `"rgb(239,68,68)"`).
    /// Defaults to empty; backfilled after deserialisation.
    #[serde(default)]
    pub color: String,

    /// 2-3 sentence persona definition.  Re-injected at the top of every
    /// turn to anchor the model's identity.
    pub persona: String,

    /// What unique angle this agent brings, expressed as a single sentence.
    pub perspective: String,

    /// Behavioural contentiousness on a `[0.0, 1.0]` scale.
    ///
    /// - `0.0` – fully collaborative (seek common ground)
    /// - `1.0` – maximally adversarial (devil's advocate)
    ///
    /// The raw float is stored for UI slider binding.  Prompt assembly
    /// maps it to a discrete instruction string via
    /// [`crate::council::prompts::contentiousness_to_instruction`].
    ///
    /// The alias covers a common LLM typo (`"contententiousness"`).
    #[serde(alias = "contententiousness")]
    pub contentiousness: f32,

    /// Optional allowlist of tool names this agent may use.
    ///
    /// - `None` → agent may use **all** tools available to the session.
    /// - `Some(vec![])` → agent may use **no** tools.
    /// - `Some(vec!["web_search"])` → agent may only use `web_search`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_filter: Option<Vec<String>>,
}

// ─── council-level config ────────────────────────────────────────────────────

/// Full configuration for a council deliberation run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CouncilConfig {
    /// The agents that will participate in the debate.
    pub agents: Vec<CouncilAgent>,

    /// The user's question or topic that the council will deliberate on.
    pub topic: String,

    /// Number of debate rounds before synthesis.
    pub rounds: u32,

    /// Optional guidance injected into the synthesis prompt to steer the
    /// final answer's focus (e.g. "prioritise actionable recommendations").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub synthesis_guidance: Option<String>,
}

// ─── suggested council (returned by /api/council/suggest) ────────────────────

/// The LLM's suggested council composition, returned from the designer
/// endpoint.  Every field is user-editable before the run begins.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestedCouncil {
    /// Suggested agents.
    pub agents: Vec<CouncilAgent>,

    /// Suggested number of debate rounds.
    ///
    /// Defaults to `0` when absent (fill responses only return one agent
    /// and may omit council-level fields).
    #[serde(default)]
    pub rounds: u32,

    /// Suggested synthesis guidance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub synthesis_guidance: Option<String>,
}

/// Default agent colours, cycled when the LLM omits `color`.
const DEFAULT_AGENT_COLORS: &[&str] = &[
    "#3b82f6", // blue
    "#ef4444", // red
    "#10b981", // emerald
    "#f59e0b", // amber
    "#8b5cf6", // violet
    "#ec4899", // pink
    "#06b6d4", // cyan
    "#f97316", // orange
];

impl SuggestedCouncil {
    /// Convert into a [`CouncilConfig`] by supplying the user's topic.
    pub fn into_config(self, topic: String) -> CouncilConfig {
        CouncilConfig {
            agents: self.agents,
            topic,
            rounds: self.rounds,
            synthesis_guidance: self.synthesis_guidance,
        }
    }

    /// Fill in any `id` or `color` fields that the LLM left empty.
    ///
    /// Existing non-empty values are preserved so that stable IDs from
    /// a prior suggestion survive a refinement round.
    pub fn backfill_defaults(&mut self) {
        for (i, agent) in self.agents.iter_mut().enumerate() {
            if agent.id.is_empty() {
                agent.id = format!("{}-{}", agent.name.to_lowercase().replace(' ', "-"), i + 1,);
            }
            if agent.color.is_empty() {
                agent.color = DEFAULT_AGENT_COLORS[i % DEFAULT_AGENT_COLORS.len()].to_string();
            }
        }
    }
}

// ─── validation ──────────────────────────────────────────────────────────────

/// Clamp `contentiousness` into `[0.0, 1.0]`.
#[must_use]
pub const fn clamp_contentiousness(value: f32) -> f32 {
    if value < 0.0 {
        0.0
    } else if value > 1.0 {
        1.0
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_within_range() {
        assert!((clamp_contentiousness(0.5) - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn clamp_below_zero() {
        assert!((clamp_contentiousness(-0.3) - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn clamp_above_one() {
        assert!((clamp_contentiousness(1.7) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn round_trip_council_agent_json() {
        let agent = CouncilAgent {
            id: "skeptic-1".into(),
            name: "Skeptic".into(),
            color: "#ef4444".into(),
            persona: "A rigorous critic who demands evidence.".into(),
            perspective: "Challenges assumptions.".into(),
            contentiousness: 0.8,
            tool_filter: Some(vec!["web_search".into()]),
        };
        let json = serde_json::to_string(&agent).unwrap();
        let back: CouncilAgent = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "skeptic-1");
        assert!((back.contentiousness - 0.8).abs() < f32::EPSILON);
        assert_eq!(back.tool_filter.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn council_agent_without_tool_filter() {
        let json = r##"{
            "id": "a",
            "name": "A",
            "color": "#000",
            "persona": "p",
            "perspective": "v",
            "contentiousness": 0.5
        }"##;
        let agent: CouncilAgent = serde_json::from_str(json).unwrap();
        assert!(agent.tool_filter.is_none());
    }

    #[test]
    fn backfill_preserves_existing_ids_and_colors() {
        let mut council = SuggestedCouncil {
            agents: vec![
                CouncilAgent {
                    id: "kept-id".into(),
                    name: "Kept".into(),
                    color: "#aaa".into(),
                    persona: String::new(),
                    perspective: String::new(),
                    contentiousness: 0.5,
                    tool_filter: None,
                },
                CouncilAgent {
                    id: String::new(),
                    name: "New Agent".into(),
                    color: String::new(),
                    persona: String::new(),
                    perspective: String::new(),
                    contentiousness: 0.3,
                    tool_filter: None,
                },
            ],
            rounds: 2,
            synthesis_guidance: None,
        };
        council.backfill_defaults();
        assert_eq!(council.agents[0].id, "kept-id");
        assert_eq!(council.agents[0].color, "#aaa");
        assert_eq!(council.agents[1].id, "new-agent-2");
        assert!(!council.agents[1].color.is_empty());
    }
}
