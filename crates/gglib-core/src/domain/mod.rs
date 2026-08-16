#![doc = include_str!("README.md")]
pub(crate) mod admission;
pub mod agent;
pub mod benchmark;
pub(crate) mod cache_budget;
pub mod capabilities;
pub mod capability_tags;
pub mod chat;
pub mod defects;
pub mod dialect;
pub(crate) mod generation_config;
pub mod gguf;
pub(crate) mod inference;
pub mod inference_profile;
pub(crate) mod kv_estimate;
pub(crate) mod kv_memory;
pub(crate) mod launch_narration;
pub mod mcp;
mod model;
pub(crate) mod model_naming;
pub(crate) mod model_sampling;
pub(crate) mod query;
pub mod recommendation;
pub(crate) mod residency;
pub(crate) mod runtime_capabilities;
pub(crate) mod sampling_provenance;
mod server_config;
pub mod slot_eviction;

// Re-export model types at the domain level for convenience
pub use model::{Model, ModelFile, ModelFilterOptions, NewModel, NewModelFile};

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
pub use capability_tags::is_reasoning;
pub use inference::{DefaultsOrigin, FieldIssue, InferenceConfig, ModelSamplingContext};
pub use inference_profile::{InferenceProfile, builtin_templates};

// Re-export sampling provenance types at the domain level for convenience
pub use sampling_provenance::{FieldSources, ParamSource, SamplingLayer};

// Re-export runtime (llama-server) capability detection at the domain level
pub use runtime_capabilities::{RuntimeCapabilities, RuntimeFlags};

// Re-export KV estimation helpers at the domain level for convenience
pub use kv_estimate::{
    KvElemsPerToken, estimate_kv_bytes_for_context, estimate_kv_elems_per_token, kv_bytes_per_token,
};

// Re-export KV memory-shape detection at the domain level for convenience
pub use kv_memory::{kv_cache_layer_count, kv_memory_is_partial};
pub use model_sampling::{
    MODEL_SAMPLING_KEYS, ModelSamplingDefault, ModelSamplingDefaults, SamplingOverride,
};

// Re-export launch narration types at the domain level for convenience
pub use launch_narration::{LaunchDecision, LaunchNarration, format_gib, format_mib_as_gib};

// Re-export the first-run model recommendation at the domain level.
pub use recommendation::{BudgetSource, Recommendation, recommend};

// Re-export admission-control telemetry at the domain level for convenience
pub use admission::{
    AdmissionSnapshot, QueuedModelSnapshot, ResidentSlotSnapshot, SecondarySlotStatus,
};

// Re-export the second-VRAM-slot decision at the domain level for convenience
pub use residency::{
    RESIDENCY_UTILISATION, SecondarySlotDecision, SlotFootprint, decide_secondary_slot,
};
pub use server_config::ServerConfig;

// Re-export cache-RAM budget math at the domain level for convenience
pub use cache_budget::{
    CACHE_RAM_UNKNOWN_KV_ALLOWANCE_BYTES, CacheRamHealth, classify_cache_ram,
    compute_auto_cache_ram_mb,
};

// Re-export slot eviction helpers at the domain level for convenience
pub use slot_eviction::{SlotFileMeta, compute_auto_disk_budget_bytes, select_evictions};

// Re-export MCP types at the domain level for convenience
pub use mcp::{
    McpEnvEntry, McpLifecycle, McpServer, McpServerConfig, McpServerStatus, McpServerType, McpTool,
    McpToolResult, NewMcpServer, ToolIndex,
};

// Re-export chat types at the domain level for convenience
pub use chat::{
    Conversation, ConversationUpdate, Message, MessageRole, NewConversation, NewMessage,
};

// Re-export GGUF types at the domain level for convenience
pub use gguf::{CapabilityFlags, GgufCapabilities, GgufMetadata, GgufValue, RawMetadata};

// Re-export dialect types at the domain level for convenience
pub use dialect::{BodyCodec, DialectSpec, EmissionProfile};

// Re-export model-naming types at the domain level for convenience
pub use model_naming::{NameSource, repo_short_name, resolve_model_name, strip_gguf_suffix};

// Re-export agent types at the domain level for convenience
pub use agent::{
    AGENT_EVENT_CHANNEL_CAPACITY, AgentConfig, AgentEvent, AgentMessage, AssistantContent,
    DEFAULT_MAX_ITERATIONS, DEFAULT_MAX_STAGNATION_STEPS, LlmStreamEvent, LoopDetector,
    MAX_ITERATIONS_CEILING, MAX_PARALLEL_TOOLS_CEILING, MAX_TOOL_TIMEOUT_MS_CEILING,
    StagnationDetector, ToolCall, ToolDefinition, ToolResult,
};

// Re-export capability types at the domain level for convenience
pub use capabilities::capabilities_from_architecture;
pub use capabilities::infer_from_chat_template;
pub use capabilities::{
    ChatMessage, MessageContent, ModelCapabilities, transform_messages_for_capabilities,
};
pub use generation_config::generation_config_candidates;
pub use generation_config::parse_generation_config;
pub use model::RangeValues;
pub use model::is_system_tag;
pub use model_naming::declared_name;
