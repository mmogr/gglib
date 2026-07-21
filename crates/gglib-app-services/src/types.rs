//! GUI-specific DTOs for frontend communication.
//!
//! These types are cross-adapter (used by both Tauri and Axum).
//! They map between domain types and frontend-friendly representations.

use gglib_core::domain::Model;
use gglib_core::domain::mcp::McpLifecycle;
use gglib_core::ports::ProcessHandle;
use serde::{Deserialize, Serialize};

// ============================================================================
// HuggingFace Browser Types
// ============================================================================

/// Summary of a HuggingFace model from the search API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HfModelSummary {
    /// Model ID (e.g., "TheBloke/Llama-2-7B-GGUF")
    pub id: String,
    /// Human-readable model name (derived from id)
    pub name: String,
    /// Author/organization (e.g., "TheBloke")
    pub author: Option<String>,
    /// Total download count
    pub downloads: u64,
    /// Like count
    pub likes: u64,
    /// Last modified timestamp
    pub last_modified: Option<String>,
    /// Total parameter count in billions (from safetensors.total)
    pub parameters_b: Option<f64>,
    /// Model description/README excerpt
    pub description: Option<String>,
    /// Model tags
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Sort field options for HuggingFace model search.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HfSortField {
    #[default]
    Downloads,
    Likes,
    Modified,
    Created,
    #[serde(rename = "id")]
    Alphabetical,
}

/// Request for searching HuggingFace models.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HfSearchRequest {
    pub query: Option<String>,
    pub min_params_b: Option<f64>,
    pub max_params_b: Option<f64>,
    pub page: u32,
    pub limit: u32,
    #[serde(default)]
    pub sort_by: HfSortField,
    #[serde(default)]
    pub sort_ascending: bool,
}

impl Default for HfSearchRequest {
    fn default() -> Self {
        Self {
            query: None,
            min_params_b: None,
            max_params_b: None,
            page: 0,
            limit: 30,
            sort_by: HfSortField::default(),
            sort_ascending: false,
        }
    }
}

/// Response from HuggingFace model search.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HfSearchResponse {
    pub models: Vec<HfModelSummary>,
    pub has_more: bool,
    pub page: u32,
    pub total_count: Option<u64>,
}

/// Information about a specific quantization variant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HfQuantization {
    pub name: String,
    pub file_path: String,
    pub size_bytes: u64,
    pub size_mb: f64,
    pub is_sharded: bool,
    pub shard_count: Option<u32>,
}

/// Response containing available quantizations for a model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HfQuantizationsResponse {
    pub model_id: String,
    pub quantizations: Vec<HfQuantization>,
}

/// Response for tool/function calling support detection.
///
/// Used for both HuggingFace model metadata and local running server queries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSupportResponse {
    pub supports_tool_calls: bool,
    pub confidence: f32,
    pub detected_format: Option<String>,
}

impl From<gglib_core::ports::ToolSupportDetection> for ToolSupportResponse {
    fn from(detection: gglib_core::ports::ToolSupportDetection) -> Self {
        Self {
            supports_tool_calls: detection.supports_tool_calling,
            confidence: detection.confidence,
            detected_format: detection.detected_format.map(|f| f.to_string()),
        }
    }
}

// ============================================================================
// GUI Model Types
// ============================================================================

/// Frontend-friendly model structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiModel {
    pub id: i64,
    pub name: String,
    pub file_path: String,
    pub param_count_b: f64,
    pub architecture: Option<String>,
    pub quantization: Option<String>,
    pub context_length: Option<u64>,
    pub added_at: String,
    pub hf_repo_id: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub is_serving: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inference_defaults: Option<gglib_core::domain::InferenceConfig>,
    /// Per-model server defaults (port, URL overrides, etc.).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_defaults: Option<gglib_core::domain::ServerConfig>,
    /// Capability flags stored for this model.
    ///
    /// Serialized as a `u32` bit-field.  The frontend receives this value
    /// and may display individual flags; the `PATCH /api/models/{id}/capabilities`
    /// endpoint lets the user override them.
    #[serde(default)]
    pub capabilities: gglib_core::ModelCapabilities,
    /// Denormalised benchmark summary (speed badges).
    ///
    /// `None` if the model has never been benchmarked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub benchmark_summary: Option<gglib_core::domain::benchmark::ModelBenchmarkSummary>,
}

impl GuiModel {
    /// Convert a domain Model to GuiModel format.
    pub fn from_model(model: Model, is_serving: bool, port: Option<u16>) -> Self {
        Self {
            id: model.id,
            name: model.name,
            file_path: model.file_path.to_string_lossy().to_string(),
            param_count_b: model.param_count_b,
            architecture: model.architecture,
            quantization: model.quantization,
            context_length: model.context_length,
            added_at: model.added_at.format("%Y-%m-%d %H:%M:%S").to_string(),
            hf_repo_id: model.hf_repo_id,
            tags: model.tags,
            is_serving,
            port,
            inference_defaults: model.inference_defaults,
            server_defaults: model.server_defaults,
            capabilities: model.capabilities,
            benchmark_summary: model.benchmark_summary,
        }
    }

    /// Convert from Model with default serving status (not serving).
    pub fn from_domain(model: Model) -> Self {
        Self::from_model(model, false, None)
    }
}

impl From<Model> for GuiModel {
    fn from(model: Model) -> Self {
        Self::from_domain(model)
    }
}

// ============================================================================
// Model Inspect DTO
// ============================================================================

/// Complete model details for the inspect view.
///
/// This is a superset of [`GuiModel`] that includes all fields from the domain
/// [`Model`], including raw GGUF metadata, MoE topology, and full HuggingFace
/// provenance.  It is the single shared contract consumed by:
///
/// - CLI: `gglib model inspect` (human-readable or `--json`)
/// - Axum: `GET /api/models/:id/detail`
/// - GUI frontend: model detail panel
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelDetailDto {
    // ── Core identity ─────────────────────────────────────────────────────────
    /// Database ID of the model.
    pub id: i64,
    /// Human-readable name.
    pub name: String,
    /// Absolute path to the GGUF file on disk.
    pub file_path: String,
    /// Parameter count in billions.
    pub param_count_b: f64,
    /// Model architecture (e.g. `"llama"`, `"mistral"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub architecture: Option<String>,
    /// Quantization type (e.g. `"Q4_K_M"`, `"F16"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantization: Option<String>,
    /// Maximum context length in tokens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_length: Option<u64>,
    // ── MoE topology (omitted for non-MoE models) ─────────────────────────────
    /// Total number of experts (MoE models only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expert_count: Option<u32>,
    /// Experts activated per token (MoE models only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expert_used_count: Option<u32>,
    /// Shared experts that are always active (MoE models only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expert_shared_count: Option<u32>,
    // ── HuggingFace provenance ────────────────────────────────────────────────
    /// HuggingFace repository ID (e.g. `"bartowski/Llama-3.1-8B-GGUF"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hf_repo_id: Option<String>,
    /// Original filename on HuggingFace Hub.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hf_filename: Option<String>,
    /// Git commit SHA from HuggingFace Hub.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hf_commit_sha: Option<String>,
    /// When the model was downloaded from HuggingFace (`"%Y-%m-%d %H:%M:%S"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub download_date: Option<String>,
    /// Last time an update check was performed (`"%Y-%m-%d %H:%M:%S"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_update_check: Option<String>,
    // ── Organisation ──────────────────────────────────────────────────────────
    /// User-defined and auto-generated tags.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Capability flags serialized as a `u32` bit-field.
    #[serde(default)]
    pub capabilities: gglib_core::ModelCapabilities,
    // ── Inference defaults ────────────────────────────────────────────────────
    /// Per-model inference parameter overrides.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inference_defaults: Option<gglib_core::domain::InferenceConfig>,
    // ── Timestamps ────────────────────────────────────────────────────────────
    /// When the model was first added to the database (`"%Y-%m-%d %H:%M:%S"`).
    pub added_at: String,
    // ── Serving status ────────────────────────────────────────────────────────
    /// Whether the model is currently being served.
    #[serde(default)]
    pub is_serving: bool,
    /// Port the model is served on, if currently serving.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    // ── Raw GGUF key-value pairs ──────────────────────────────────────────────
    /// All raw key-value pairs stored from the GGUF file.
    ///
    /// Presentation layers decide whether to surface this.  The CLI gates it
    /// behind `--metadata`; the GUI may show it in a collapsible panel.
    pub metadata: std::collections::HashMap<String, String>,
}

impl ModelDetailDto {
    /// Convert a domain [`Model`] to [`ModelDetailDto`].
    ///
    /// `is_serving` and `port` are injected by the service layer, which has
    /// access to the running-process list.  Pass `false` / `None` from
    /// contexts where serving state is not relevant (e.g. the CLI).
    pub fn from_model(model: Model, is_serving: bool, port: Option<u16>) -> Self {
        Self {
            id: model.id,
            name: model.name,
            file_path: model.file_path.to_string_lossy().to_string(),
            param_count_b: model.param_count_b,
            architecture: model.architecture,
            quantization: model.quantization,
            context_length: model.context_length,
            expert_count: model.expert_count,
            expert_used_count: model.expert_used_count,
            expert_shared_count: model.expert_shared_count,
            hf_repo_id: model.hf_repo_id,
            hf_filename: model.hf_filename,
            hf_commit_sha: model.hf_commit_sha,
            download_date: model
                .download_date
                .map(|d| d.format("%Y-%m-%d %H:%M:%S").to_string()),
            last_update_check: model
                .last_update_check
                .map(|d| d.format("%Y-%m-%d %H:%M:%S").to_string()),
            tags: model.tags,
            capabilities: model.capabilities,
            inference_defaults: model.inference_defaults,
            added_at: model.added_at.format("%Y-%m-%d %H:%M:%S").to_string(),
            is_serving,
            port,
            metadata: model.metadata,
        }
    }
}

// ============================================================================
// Server Types
// ============================================================================

/// Request body for starting a server.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct StartServerRequest {
    pub context_length: Option<u64>,
    pub port: Option<u16>,
    #[serde(default)]
    pub jinja: Option<bool>,
    #[serde(default)]
    pub reasoning_format: Option<String>,
    /// Number of MTP draft tokens (`--spec-draft-n-max`).
    ///
    /// `None` = auto-detect from model tags.  `Some(0)` = explicitly disabled.
    /// `Some(n > 0)` = explicitly enable with n tokens.
    #[serde(default)]
    pub mtp_draft_n_max: Option<u32>,
    /// Minimum acceptance probability for MTP draft tokens (`--spec-draft-p-min`).
    ///
    /// Only meaningful when `mtp_draft_n_max` is `Some`.  Defaults to `0.75`.
    #[serde(default)]
    pub mtp_draft_p_min: Option<f32>,
    /// Inference parameters for this serve session (overrides model/global defaults).
    #[serde(default)]
    pub inference_params: Option<gglib_core::domain::InferenceConfig>,
}

/// Response for starting a server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartServerResponse {
    pub port: u16,
    pub message: String,
}

/// Information about a running model server (GUI DTO).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    pub model_id: i64,
    pub model_name: String,
    pub pid: Option<u32>,
    pub port: u16,
    pub started_at: u64,
}

impl ServerInfo {
    /// Create from a ProcessHandle.
    pub fn from_handle(handle: &ProcessHandle) -> Self {
        Self {
            model_id: handle.model_id,
            model_name: handle.model_name.clone(),
            pid: handle.pid,
            port: handle.port,
            started_at: handle.started_at,
        }
    }
}

// ============================================================================
// Model Request Types
// ============================================================================

/// Request body for adding a model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddModelRequest {
    pub file_path: String,
}

/// Request body for removing a model.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RemoveModelRequest {
    #[serde(default)]
    pub force: bool,
}

/// Request body for updating a model.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpdateModelRequest {
    pub name: Option<String>,
    pub quantization: Option<String>,
    pub file_path: Option<String>,
    pub inference_defaults: Option<gglib_core::domain::InferenceConfig>,
    /// Per-model server startup defaults.
    /// - Some(Some(config)) — set/replace the model's server defaults
    /// - Some(None) — clear the override (NULL in DB, revert to global default)
    /// - None — don't touch this field (key omitted from payload)
    #[serde(default, with = "serde_with::rust::double_option")]
    pub server_defaults: Option<Option<gglib_core::domain::ServerConfig>>,
}

/// Request body for overriding a model's capability flags.
///
/// Each field independently sets (`true`) or clears (`false`) one flag.
/// `None` means "leave this flag unchanged".  This lets callers toggle a
/// single flag without knowing the current state of every other flag.
///
/// # Example
///
/// Force strict-turn coalescing on for a model whose GGUF shipped without
/// a chat template:
///
/// ```json
/// { "requiresStrictTurns": true }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SetCapabilitiesRequest {
    /// Override whether the model supports a `system` role in the chat template.
    pub supports_system_role: Option<bool>,
    /// Override whether the model requires strict user/assistant turn alternation.
    pub requires_strict_turns: Option<bool>,
    /// Override whether the model supports tool/function calling.
    pub supports_tool_calls: Option<bool>,
    /// Override whether the model produces reasoning/thinking output.
    pub supports_reasoning: Option<bool>,
}

// ============================================================================
// Settings Types
// ============================================================================

/// Current configuration for the models directory shown in settings UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelsDirectoryInfo {
    pub path: String,
    pub source: String,
    pub default_path: String,
    pub exists: bool,
    pub writable: bool,
}

/// Application settings for the settings UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub default_download_path: Option<String>,
    pub default_context_size: Option<u64>,
    pub proxy_port: Option<u16>,
    pub llama_base_port: Option<u16>,
    pub max_download_queue_size: Option<u32>,
    pub show_memory_fit_indicators: Option<bool>,
    pub max_tool_iterations: Option<u32>,
    pub max_stagnation_steps: Option<u32>,
    /// Default model ID for quick commands (e.g., `gglib question`).
    pub default_model_id: Option<i64>,
    pub inference_defaults: Option<gglib_core::domain::InferenceConfig>,
    /// Named sampling profiles, selectable per request as `{model}:{profile}`.
    pub inference_profiles: Option<Vec<gglib_core::domain::InferenceProfile>>,
    // Setup wizard
    pub setup_completed: Option<bool>,
    // Title generation
    pub title_generation_prompt: Option<String>,
}

/// Request body for updating application settings.
///
/// Every field is `Option<Option<T>>` with `serde_with::rust::double_option`
/// so an explicit JSON `null` (clear the setting) is distinguished from an
/// omitted key (leave unchanged) — the same pattern used by
/// [`UpdateModelRequest::server_defaults`].
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSettingsRequest {
    #[serde(default, with = "serde_with::rust::double_option")]
    pub default_download_path: Option<Option<String>>,
    #[serde(default, with = "serde_with::rust::double_option")]
    pub default_context_size: Option<Option<u64>>,
    #[serde(default, with = "serde_with::rust::double_option")]
    pub proxy_port: Option<Option<u16>>,
    #[serde(default, with = "serde_with::rust::double_option")]
    pub llama_base_port: Option<Option<u16>>,
    #[serde(default, with = "serde_with::rust::double_option")]
    pub max_download_queue_size: Option<Option<u32>>,
    #[serde(default, with = "serde_with::rust::double_option")]
    pub show_memory_fit_indicators: Option<Option<bool>>,
    #[serde(default, with = "serde_with::rust::double_option")]
    pub max_tool_iterations: Option<Option<u32>>,
    #[serde(default, with = "serde_with::rust::double_option")]
    pub max_stagnation_steps: Option<Option<u32>>,
    /// Default model ID for quick commands (e.g., `gglib question`).
    #[serde(default, with = "serde_with::rust::double_option")]
    pub default_model_id: Option<Option<i64>>,
    #[serde(default, with = "serde_with::rust::double_option")]
    pub inference_defaults: Option<Option<gglib_core::domain::InferenceConfig>>,
    /// Replaces the whole profile list. `null` clears it; an omitted key leaves
    /// it untouched, so a client updating an unrelated setting cannot drop
    /// profiles it never knew about.
    #[serde(default, with = "serde_with::rust::double_option")]
    pub inference_profiles: Option<Option<Vec<gglib_core::domain::InferenceProfile>>>,
    // Setup wizard
    #[serde(default, with = "serde_with::rust::double_option")]
    pub setup_completed: Option<Option<bool>>,
    // Title generation
    #[serde(default, with = "serde_with::rust::double_option")]
    pub title_generation_prompt: Option<Option<String>>,
}

// ============================================================================
// MCP Types
// ============================================================================

/// MCP server DTO for serialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerDto {
    pub id: i64,
    pub name: String,
    pub server_type: String,
    pub config: McpServerConfigDto,
    pub enabled: bool,
    pub lifecycle: McpLifecycle,
    pub env: Vec<McpEnvEntryDto>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_connected_at: Option<String>,
    /// Whether the server configuration is valid
    pub is_valid: bool,
    /// Last validation or runtime error
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

/// MCP server configuration DTO.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfigDto {
    /// Command/basename to resolve (e.g., "npx" or "/usr/local/bin/python3")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// Cached absolute path (auto-resolved from command)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_path_cache: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    /// Working directory (must be absolute if specified)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,
    /// Additional PATH entries for child process
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path_extra: Option<String>,
    /// URL for SSE connection (required for sse)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// MCP environment variable DTO.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpEnvEntryDto {
    pub key: String,
    pub value: String,
}

/// MCP server status DTO.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum McpServerStatusDto {
    Stopped,
    Starting,
    Running,
    Error(String),
}

/// MCP server info for GUI display (nested structure matching TS expectations).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerInfo {
    pub server: McpServerDto,
    pub status: McpServerStatusDto,
    #[serde(default)]
    pub tools: Vec<McpToolInfo>,
}

/// Request to create a new MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMcpServerRequest {
    pub name: String,
    pub server_type: String,
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    pub working_dir: Option<String>,
    pub path_extra: Option<String>,
    pub url: Option<String>,
    #[serde(default)]
    pub env: Vec<McpEnvEntryDto>,
    #[serde(default)]
    pub lifecycle: McpLifecycle,
}

/// Request to update an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateMcpServerRequest {
    pub name: Option<String>,
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    pub working_dir: Option<String>,
    pub path_extra: Option<String>,
    pub url: Option<String>,
    pub env: Option<Vec<McpEnvEntryDto>>,
    pub enabled: Option<bool>,
    pub lifecycle: Option<McpLifecycle>,
}

/// MCP tool information for GUI display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolInfo {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: Option<serde_json::Value>,
    /// Human-readable display title from MCP `annotations.title`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// Request to call an MCP tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolCallRequest {
    pub tool_name: String,
    pub arguments: std::collections::HashMap<String, serde_json::Value>,
}

/// Response from an MCP tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolCallResponse {
    pub success: bool,
    pub data: Option<serde_json::Value>,
    pub error: Option<String>,
}

// ============================================================================
// Server Log Types
// ============================================================================

// Re-export from gglib-runtime for cross-adapter use
pub use gglib_runtime::ServerLogEntry;

#[cfg(test)]
mod update_model_request_tests {
    //! JSON-boundary tests for `UpdateModelRequest.server_defaults`.
    //!
    //! These deserialize raw JSON strings (rather than constructing the
    //! struct directly in Rust) to prove the `serde_with::rust::double_option`
    //! wiring actually distinguishes "field omitted" from "field explicitly
    //! null" at the layer where it matters — every other test for this
    //! feature bypassed serde entirely and would not have caught the
    //! original bug (double `Option` collapsing `null` into "omitted").

    use super::UpdateModelRequest;
    use gglib_core::domain::ServerConfig;

    #[test]
    fn server_defaults_omitted_key_is_none() {
        let req: UpdateModelRequest = serde_json::from_str("{}").unwrap();
        assert_eq!(
            req.server_defaults, None,
            "omitted key must resolve to None (no-op / don't touch)"
        );
    }

    #[test]
    fn server_defaults_explicit_null_is_some_none() {
        let req: UpdateModelRequest = serde_json::from_str(r#"{"serverDefaults": null}"#).unwrap();
        assert_eq!(
            req.server_defaults,
            Some(None),
            "explicit null must resolve to Some(None) (clear the override)"
        );
    }

    #[test]
    fn server_defaults_populated_object_is_some_some() {
        let req: UpdateModelRequest =
            serde_json::from_str(r#"{"serverDefaults": {"contextLength": 8192}}"#).unwrap();
        assert_eq!(
            req.server_defaults,
            Some(Some(ServerConfig {
                context_length: Some(8192)
            })),
            "populated object must resolve to Some(Some(config))"
        );
    }
}
