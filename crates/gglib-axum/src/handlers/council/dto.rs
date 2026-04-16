//! Request / response DTOs for the council endpoints.

use serde::{Deserialize, Serialize};

use gglib_agent::council::{CouncilConfig, SuggestedCouncil};

/// Request body for `POST /api/council/suggest`.
#[derive(Debug, Deserialize)]
pub struct CouncilSuggestRequest {
    /// Port of the llama-server instance to use for design.
    pub port: u16,

    /// The user's topic or question for the council.
    pub topic: String,

    /// How many agents to suggest (default: 3).
    #[serde(default = "default_agent_count")]
    pub agent_count: u32,

    /// Optional model-name override forwarded to llama-server.
    #[serde(default)]
    pub model: Option<String>,
}

fn default_agent_count() -> u32 {
    3
}

/// Response body for `POST /api/council/suggest`.
#[derive(Debug, Serialize)]
pub struct CouncilSuggestResponse {
    /// The LLM-suggested council.
    #[serde(flatten)]
    pub council: SuggestedCouncil,
}

/// Request body for `POST /api/council/run`.
#[derive(Debug, Deserialize)]
pub struct CouncilRunRequest {
    /// Port of the llama-server instance to drive.
    pub port: u16,

    /// Full council configuration (agents, topic, rounds, etc.).
    pub council: CouncilConfig,

    /// Optional model-name override forwarded to llama-server.
    #[serde(default)]
    pub model: Option<String>,

    /// Optional loop tuning — same shape as the agent chat endpoint.
    pub config: Option<crate::handlers::agent::AgentRequestConfig>,
}

// ── Re-exports for handler convenience ───────────────────────────────────────

pub(crate) use gglib_agent::council::{COUNCIL_EVENT_CHANNEL_CAPACITY, CouncilEvent};
