//! Council of Agents — multi-agent deliberation with structured debate.
//!
//! This module orchestrates multiple LLM-backed agents through rounds of
//! debate on a user's topic, then produces a synthesised answer.  Each
//! agent runs via the existing [`AgentLoop`](crate::AgentLoop) — this
//! module adds orchestration, not a new loop implementation.
//!
//! # Module layout
//!
//! | File              | Responsibility                                      |
//! |-------------------|-----------------------------------------------------|
//! | `config.rs`       | `CouncilConfig`, `CouncilAgent`, `SuggestedCouncil` |
//! | `events.rs`       | `CouncilEvent` SSE enum (wire format)               |
//! | `prompts.rs`      | Prompt templates + contentiousness mapping          |
//! | `state.rs`        | Round/contribution accumulator                      |
//! | `history.rs`      | Per-turn context builder (identity + transcript)    |
//! | `stream_bridge.rs`| `AgentEvent` → `CouncilEvent` mapper                |
//! | `round.rs`        | Sequential round execution (per-agent turn driver)  |
//! | `synthesis.rs`    | Synthesis pass (transcript → unified answer)        |
//! | `judge.rs`        | Post-round judge + adaptive early stopping          |
//! | `orchestrator.rs` | Slim coordinator (rounds → judge → synthesis)       |
//! | `suggest.rs`      | `suggest_council()` — shared suggest orchestration  |

pub mod config;
pub mod events;
pub mod history;
pub mod orchestrator;
pub mod prompts;
mod judge;
mod round;
pub mod state;
pub mod stream_bridge;
pub mod suggest;
mod synthesis;

pub use config::{CouncilAgent, CouncilConfig, JudgeConfig, SuggestedCouncil};
pub use events::{COUNCIL_EVENT_CHANNEL_CAPACITY, CouncilEvent};
pub use orchestrator::run as run_council;
pub use prompts::{contentiousness_tier_label, contentiousness_to_instruction};
pub use state::{AgentContribution, CouncilState, extract_core_claim};
pub use stream_bridge::{bridge_agent_events, emit_turn_complete};
pub use suggest::suggest_council;
