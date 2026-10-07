//! GUI-specific DTOs for frontend communication.
//!
//! These types are cross-adapter (used by both Tauri and Axum).
//! They map between domain types and frontend-friendly representations.

use gglib_core::domain::Model;
use gglib_core::domain::mcp::McpLifecycle;
use gglib_core::ports::ProcessHandle;
use serde::{Deserialize, Serialize};

#[path = "types_hf.rs"]
mod types_hf;
pub use types_hf::{
    HfModelSummary, HfProjector, HfQuantization, HfQuantizationsResponse, HfSearchRequest,
    HfSearchResponse, HfSortField, ToolSupportResponse,
};
#[path = "types_model_update.rs"]
mod types_model_update;
pub use types_model_update::UpdateModelRequest;

// ============================================================================
// GUI Model Types
// ============================================================================

/// Frontend-friendly model structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct GuiModel {
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub id: i64,
    pub name: String,
    pub file_path: String,
    pub param_count_b: f64,
    pub architecture: Option<String>,
    pub quantization: Option<String>,
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null"))]
    pub context_length: Option<u64>,
    // ── MoE topology (omitted for dense models) ───────────────────────────────
    // The list view renders *active* parameters from these, the same way the
    // inspector does off [`ModelDetailDto`]; without them it silently shows the
    // total instead.
    /// Total number of experts (`MoE` models only).
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expert_count: Option<u32>,
    /// Experts activated per token (`MoE` models only).
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expert_used_count: Option<u32>,
    /// Shared experts that are always active (`MoE` models only).
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expert_shared_count: Option<u32>,
    pub added_at: String,
    pub hf_repo_id: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub is_serving: bool,
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inference_defaults: Option<gglib_core::domain::InferenceConfig>,
    /// Whether [`Self::inference_defaults`] was set by the user or
    /// auto-detected at import time. See `gglib_core::domain::DefaultsOrigin`.
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defaults_origin: Option<gglib_core::domain::DefaultsOrigin>,
    /// Per-model server defaults (port, URL overrides, etc.).
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_defaults: Option<gglib_core::domain::ServerConfig>,
    /// Capability flags stored for this model.
    ///
    /// Serialized as a `u32` bit-field.  The frontend receives this value
    /// and may display individual flags; the `PATCH /api/models/{id}/capabilities`
    /// endpoint lets the user override them.
    ///
    /// A `bitflags` newtype over `u32`, so it crosses the wire as a bare
    /// number and cannot derive `TS` itself — TypeScript reads it through the
    /// `CAPABILITY_FLAGS` bitmask.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    #[serde(default)]
    pub capabilities: gglib_core::ModelCapabilities,
    /// Whether the model reads images: it is linked to a projector.
    #[serde(default)]
    pub image_input: bool,
    /// Denormalised benchmark summary (speed badges).
    ///
    /// `None` if the model has never been benchmarked.
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub benchmark_summary: Option<gglib_core::domain::benchmark::ModelBenchmarkSummary>,
}

impl GuiModel {
    /// Convert a domain Model to `GuiModel` format.
    pub fn from_model(model: Model, is_serving: bool, port: Option<u16>) -> Self {
        Self {
            image_input: model.image_input(),
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
            added_at: model.added_at.format("%Y-%m-%d %H:%M:%S").to_string(),
            hf_repo_id: model.hf_repo_id,
            tags: model.tags,
            is_serving,
            port,
            inference_defaults: model.inference_defaults,
            defaults_origin: model.defaults_origin,
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
// Server Types
// ============================================================================

/// Request body for starting a server.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct StartServerRequest {
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null"))]
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
    /// Memory-lock the model into RAM (`--mlock`).
    #[serde(default)]
    pub mlock: bool,
}

/// Response for queueing a download: the one shape the daemon answers with
/// and the CLI reads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct QueueDownloadResponse {
    /// The download's canonical ID: its row's `id` in the queue snapshot,
    /// and its entry's in `finished` once it has ended.
    pub id: String,
}

/// Response for starting a server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct StartServerResponse {
    pub port: u16,
    pub message: String,
}

/// Information about a running model server (GUI DTO).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct ServerInfo {
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub model_id: i64,
    pub model_name: String,
    pub pid: Option<u32>,
    pub port: u16,
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub started_at: u64,
}

impl ServerInfo {
    /// Create from a `ProcessHandle`.
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
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct AddModelRequest {
    pub file_path: String,
}

/// Request body for removing a model.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct RemoveModelRequest {
    #[serde(default)]
    pub force: bool,
}

/// One projector file the inspector's picker offers for a model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ProjectorChoice {
    /// Absolute path of the file, the value an update sends back.
    pub path: String,
    /// The file's name, for the picker's label.
    pub name: String,
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
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
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

/// What a retag pass changed. Mirrors `gglib_core::services::RetagDiff`,
/// with `changed` folded in so the wire needs no method call.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct RetagResponse {
    pub changed: bool,
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub spec_changed: bool,
}

/// Whether a newer `HuggingFace` revision exists for a model.
///
/// `currentSha` is `None` when the model was imported without a recorded
/// revision. The check treats that as "an update exists" (there is no
/// baseline to compare against), so callers should present a missing
/// baseline distinctly rather than as a genuine new revision.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct UpgradeCheck {
    pub has_update: bool,
    pub current_sha: Option<String>,
    pub latest_sha: String,
}

/// The outcome of applying an upgrade.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct UpgradeOutcome {
    /// False when the model was already at the latest revision.
    pub updated: bool,
    pub latest_sha: String,
    /// The new on-disk path, present only when an upgrade ran.
    pub file_path: Option<String>,
}

// ============================================================================
// Settings Types
// ============================================================================

/// Current configuration for the models directory shown in settings UI.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct ModelsDirectoryInfo {
    pub path: String,
    pub source: String,
    pub default_path: String,
    pub exists: bool,
    pub writable: bool,
}

#[path = "types_settings.rs"]
mod types_settings;
pub use types_settings::{AppSettings, InstalledTemplates, UpdateSettingsRequest};

// ============================================================================
// MCP Types
// ============================================================================

/// MCP server DTO for serialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct McpServerDto {
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub id: i64,
    pub name: String,
    pub server_type: String,
    pub config: McpServerConfigDto,
    pub enabled: bool,
    pub lifecycle: McpLifecycle,
    pub env: Vec<McpEnvEntryDto>,
    pub created_at: String,
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_connected_at: Option<String>,
    /// Whether the server configuration is valid
    pub is_valid: bool,
    /// Last validation or runtime error
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

/// MCP server configuration DTO.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct McpServerConfigDto {
    /// Command/basename to resolve (e.g., "npx" or "/usr/local/bin/python3")
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// Cached absolute path (auto-resolved from command)
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_path_cache: Option<String>,
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    /// Working directory (must be absolute if specified)
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,
    /// Additional PATH entries for child process
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path_extra: Option<String>,
    /// URL for SSE connection (required for sse)
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// MCP environment variable DTO.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct McpEnvEntryDto {
    pub key: String,
    pub value: String,
}

/// MCP server status DTO.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "lowercase")]
pub enum McpServerStatusDto {
    Stopped,
    Starting,
    Running,
    Error(String),
}

/// MCP server info for GUI display (nested structure matching TS expectations).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct McpServerInfo {
    pub server: McpServerDto,
    pub status: McpServerStatusDto,
    #[serde(default)]
    pub tools: Vec<McpToolInfo>,
}

/// Request to create a new MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
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
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
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
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct McpToolInfo {
    pub name: String,
    pub description: Option<String>,
    /// Raw JSON Schema, passed through verbatim from the MCP server. Opaque
    /// here, so it crosses as unstructured JSON rather than a named type.
    #[cfg_attr(feature = "ts-bindings", ts(type = "Record<string, unknown> | null"))]
    pub input_schema: Option<serde_json::Value>,
    /// Human-readable display title from MCP `annotations.title`.
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// The outcome of `gglib mcp test`: did the server start, and what does it
/// offer.
///
/// A failed connection is a result, not an error — a misconfigured command is
/// the ordinary case this exists to diagnose, so `ok: false` carries the
/// reason rather than the request failing.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct McpTestResult {
    pub ok: bool,
    /// Why the connection failed. Absent when `ok`.
    pub error: Option<String>,
    /// What the server offered. Empty unless `ok`.
    pub tools: Vec<McpToolInfo>,
}

// ============================================================================
// Server Log Types
// ============================================================================

// Re-export from gglib-runtime for cross-adapter use
pub use gglib_runtime::ServerLogEntry;

#[cfg(test)]
#[path = "types_tests.rs"]
mod types_tests;

#[cfg(test)]
#[path = "types_model_dto_tests.rs"]
mod types_model_dto_tests;
