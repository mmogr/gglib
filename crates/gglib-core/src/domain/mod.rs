#![doc = include_str!("README.md")]
pub mod agent;
pub mod benchmark;
pub mod capabilities;
pub mod chat;
pub mod council;
pub mod gguf;
pub mod inference;
pub mod mcp;
mod model;
pub mod query;
mod server_config;

// Re-export model types at the domain level for convenience
pub use model::{
    Model, ModelFile, ModelFilterOptions, NewModel, NewModelFile, RangeValues, SYSTEM_TAG_PREFIX,
    is_system_tag,
};

// Re-export query types at the domain level for convenience
pub use query::{ModelListQuery, ModelSortBy, SortOrder, apply_query};

// Re-export benchmark types at the domain level for convenience
pub use benchmark::{
    BenchmarkEvent, BenchmarkModelResult, BenchmarkRun, BenchmarkRunStatus, BenchmarkRunType,
    CandidateSource, CompareConfig, ModelBenchmarkSummary, ModelCompareResult, ModelPerfResult,
    PerfConfig, ScoreWeights, SweepSpec, TaskCategory, TaskSuite, TuneCandidateResult, TuneConfig,
    TuneTask, TuneTaskResult,
};

// Re-export inference types at the domain level for convenience
pub use inference::InferenceConfig;
pub use server_config::ServerConfig;

// Re-export MCP types at the domain level for convenience
pub use mcp::{
    McpEnvEntry, McpLifecycle, McpServer, McpServerConfig, McpServerStatus, McpServerType, McpTool,
    McpToolResult, NewMcpServer, SEARCH_RESULTS_CAP, ToolIndex, ToolSummary, UpdateMcpServer,
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
    ChatMessage, MessageContent, ModelCapabilities, capabilities_from_architecture,
    infer_from_chat_template, transform_messages_for_capabilities,
};

// Re-export orchestrator types at the domain level for convenience
pub use council::{
    ApprovalKind, CouncilEvent, HitlMode, MAX_DEPTH, MAX_NODES, NodeId, NodeStatus, TaskGraph,
    TaskGraphError, TaskNode,
};
