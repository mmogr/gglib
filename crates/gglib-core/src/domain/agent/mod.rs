#![doc = include_str!("README.md")]
pub mod config;
pub(crate) mod events;
pub(crate) mod fnv1a;
pub(crate) mod loop_detection;
pub(crate) mod messages;
mod messages_serde;
pub(crate) mod stagnation;
pub mod tool_display;
pub(crate) mod tool_types;

// Re-export everything so callers continue to use `gglib_core::AgentConfig` etc.
pub use config::{
    AgentConfig, AgentConfigError, DEFAULT_MAX_ITERATIONS, DEFAULT_MAX_PARALLEL_TOOLS,
    DEFAULT_MAX_STAGNATION_STEPS, MAX_ITERATIONS_CEILING, MAX_PARALLEL_TOOLS_CEILING,
    MAX_STAGNATION_STEPS_CEILING, MAX_TOOL_TIMEOUT_MS_CEILING, MIN_CONTEXT_BUDGET_CHARS,
    MIN_TOOL_TIMEOUT_MS,
};
pub use events::{AGENT_EVENT_CHANNEL_CAPACITY, AgentEvent, LlmStreamEvent};
pub use loop_detection::{LoopDetector, batch_signature, is_observation_batch};
pub use messages::{AgentMessage, AssistantContent};
pub use stagnation::StagnationDetector;
pub use tool_types::{ToolCall, ToolDefinition, ToolResult};
