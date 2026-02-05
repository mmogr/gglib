//! GUI-specific DTOs for frontend communication.
//!
//! These types are cross-adapter (used by both Tauri and Axum).
//! They map between domain types and frontend-friendly representations.

use gglib_core::domain::Model;
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HfToolSupportResponse {
    pub supports_tool_calling: bool,
    pub confidence: f32,
    pub detected_format: Option<String>,
}

impl From<gglib_core::ports::ToolSupportDetection> for HfToolSupportResponse {
    fn from(detection: gglib_core::ports::ToolSupportDetection) -> Self {
        Self {
            supports_tool_calling: detection.supports_tool_calling,
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
// Server Types
// ============================================================================

/// Request body for starting a server.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct StartServerRequest {
    pub context_length: Option<u64>,
    pub port: Option<u16>,
    #[serde(default)]
    pub mlock: bool,
    #[serde(default)]
    pub jinja: Option<bool>,
    #[serde(default)]
    pub reasoning_format: Option<String>,
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
}

/// Request body for updating application settings.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSettingsRequest {
    pub default_download_path: Option<Option<String>>,
    pub default_context_size: Option<Option<u64>>,
    pub proxy_port: Option<Option<u16>>,
    pub llama_base_port: Option<Option<u16>>,
    pub max_download_queue_size: Option<Option<u32>>,
    pub show_memory_fit_indicators: Option<Option<bool>>,
    pub max_tool_iterations: Option<Option<u32>>,
    pub max_stagnation_steps: Option<Option<u32>>,
    /// Default model ID for quick commands (e.g., `gglib question`).
    pub default_model_id: Option<Option<i64>>,
    pub inference_defaults: Option<Option<gglib_core::domain::InferenceConfig>>,
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
    pub auto_start: bool,
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
    pub auto_start: bool,
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
    pub auto_start: Option<bool>,
}

/// MCP tool information for GUI display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolInfo {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: Option<serde_json::Value>,
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
