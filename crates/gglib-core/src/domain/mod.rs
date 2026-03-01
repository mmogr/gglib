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

pub mod agent;
pub mod capabilities;
pub mod chat;
pub mod gguf;
pub mod inference;
pub mod mcp;
mod model;

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
    AGENT_EVENT_CHANNEL_CAPACITY, AgentConfig, AgentEvent, AgentMessage, AssistantContent,
    DEFAULT_MAX_ITERATIONS, DEFAULT_MAX_PARALLEL_TOOLS, LlmStreamEvent, MAX_ITERATIONS_CEILING,
    MAX_PARALLEL_TOOLS_CEILING, MAX_TOOL_TIMEOUT_MS_CEILING, ToolCall,
    ToolDefinition, ToolResult,
};

// Re-export capability types at the domain level for convenience
pub use capabilities::{
    ChatMessage, ModelCapabilities, infer_from_chat_template, transform_messages_for_capabilities,
};
