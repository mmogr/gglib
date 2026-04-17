//! Core domain types.
//!
//! These types represent the pure domain model, independent of any
//! infrastructure concerns (database, filesystem, etc.).
//!
//! # Structure
//!
//! - `agent` - Agent loop types (`AgentConfig`, `AgentMessage`, `AgentEvent`, etc.)
//! - `model` - Model types (`Model`, `NewModel`)
//! - `mcp` - MCP server types (`McpServer`, `NewMcpServer`, etc.)
//! - `chat` - Chat conversation and message types
//! - `gguf` - GGUF metadata and capability types
//! - `capabilities` - Model capability detection and inference
//! - `thinking` - Thinking/reasoning tag parsing and streaming accumulation

pub mod agent;
pub mod capabilities;
pub mod chat;
pub mod gguf;
pub mod inference;
pub mod mcp;
mod model;
pub mod thinking;

// Re-export model types at the domain level for convenience
pub use model::{Model, ModelFile, ModelFilterOptions, NewModel, NewModelFile, RangeValues};

// Re-export inference types at the domain level for convenience
pub use inference::InferenceConfig;

// Re-export MCP types at the domain level for convenience
pub use mcp::{
    McpEnvEntry, McpServer, McpServerConfig, McpServerStatus, McpServerType, McpTool,
    McpToolResult, NewMcpServer, UpdateMcpServer,
};

// Re-export chat types at the domain level for convenience
pub use chat::{
    Conversation, ConversationUpdate, Message, MessageRole, NewConversation, NewMessage,
};

// Re-export GGUF types at the domain level for convenience
pub use gguf::{
    CapabilityFlags, GgufCapabilities, GgufMetadata, GgufValue, RawMetadata, ReasoningDetection,
    ToolCallingDetection,
};

// Re-export agent types at the domain level for convenience
pub use agent::{
    AGENT_EVENT_CHANNEL_CAPACITY, AgentConfig, AgentConfigError, AgentEvent, AgentMessage,
    AssistantContent, DEFAULT_MAX_ITERATIONS, DEFAULT_MAX_PARALLEL_TOOLS,
    DEFAULT_MAX_STAGNATION_STEPS, LlmStreamEvent, MAX_ITERATIONS_CEILING,
    MAX_PARALLEL_TOOLS_CEILING, MAX_TOOL_TIMEOUT_MS_CEILING, MIN_CONTEXT_BUDGET_CHARS,
    MIN_TOOL_TIMEOUT_MS, ToolCall, ToolDefinition, ToolResult,
};

// Re-export capability types at the domain level for convenience
pub use capabilities::{
    ChatMessage, ModelCapabilities, infer_from_chat_template, transform_messages_for_capabilities,
};

// Re-export thinking types at the domain level for convenience
pub use thinking::{
    ParsedThinkingContent, ThinkingAccumulator, ThinkingEvent, embed_thinking_content,
    format_thinking_duration, has_thinking_content, normalize_thinking_tags,
    parse_thinking_content,
};
