//! Agent loop domain types.
//!
//! These types define the core abstractions for the backend agentic loop.
//! They are pure domain primitives: no LLM backend references, no MCP types,
//! no infrastructure concerns.
//!
//! # Modules
//!
//! | Module | Contents |
//! |--------|----------|
//! | [`config`] | [`AgentConfig`] — loop control parameters |
//! | [`tool_types`] | [`ToolDefinition`], [`ToolCall`], [`ToolResult`] |
//! | [`messages`] | [`AgentMessage`] — closed conversation-turn enum |
//! | [`events`] | [`AgentEvent`] (SSE units), [`LlmStreamEvent`] (stream protocol) |
//!
//! # Design Principles
//!
//! - [`AgentMessage`] is a closed enum so the type system prevents invalid states
//!   (e.g. a `User` message carrying `tool_calls`).
//! - [`ToolDefinition`] is a dedicated type — adapter layers convert `McpTool →
//!   ToolDefinition`; the agent domain must not depend on MCP domain types.
//! - [`ToolResult`] with `success: false` is **context for the LLM**, not an error;
//!   tool failures are fed back into the conversation so the model can reason about
//!   them and retry or adjust its approach.
//! - [`AgentEvent`] is the unit of SSE emission; every observable state change in
//!   the loop corresponds to exactly one variant.

pub mod config;
pub mod events;
pub mod messages;
pub mod tool_types;

// Re-export everything so callers continue to use `gglib_core::AgentConfig` etc.
pub use config::{
    AgentConfig, DEFAULT_MAX_ITERATIONS, DEFAULT_MAX_PARALLEL_TOOLS, MAX_ITERATIONS_CEILING,
    MAX_PARALLEL_TOOLS_CEILING, MAX_TOOL_TIMEOUT_MS_CEILING,
};
pub use events::{AGENT_EVENT_CHANNEL_CAPACITY, AgentEvent, LlmStreamEvent};
pub use messages::{AgentMessage, AssistantContent};
pub use tool_types::{ToolCall, ToolDefinition, ToolResult};
