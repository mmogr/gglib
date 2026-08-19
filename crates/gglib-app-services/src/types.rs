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
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct HfModelSummary {
    /// Model ID (e.g., "TheBloke/Llama-2-7B-GGUF")
    pub id: String,
    /// Human-readable model name (derived from id)
    pub name: String,
    /// Author/organization (e.g., "TheBloke")
    pub author: Option<String>,
    /// Total download count
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub downloads: u64,
    /// Like count
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
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
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
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
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
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
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct HfSearchResponse {
    pub models: Vec<HfModelSummary>,
    pub has_more: bool,
    pub page: u32,
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null"))]
    pub total_count: Option<u64>,
}

/// Information about a specific quantization variant.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct HfQuantization {
    pub name: String,
    pub file_path: String,
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub size_bytes: u64,
    pub size_mb: f64,
    pub is_sharded: bool,
    pub shard_count: Option<u32>,
}

/// Response containing available quantizations for a model.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct HfQuantizationsResponse {
    pub model_id: String,
    pub quantizations: Vec<HfQuantization>,
}

/// Response for tool/function calling support detection.
///
/// Used for both HuggingFace model metadata and local running server queries.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
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
    /// Total number of experts (MoE models only).
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expert_count: Option<u32>,
    /// Experts activated per token (MoE models only).
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expert_used_count: Option<u32>,
    /// Shared experts that are always active (MoE models only).
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
    /// Denormalised benchmark summary (speed badges).
    ///
    /// `None` if the model has never been benchmarked.
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
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
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ModelDetailDto {
    // ── Core identity ─────────────────────────────────────────────────────────
    /// Database ID of the model.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub id: i64,
    /// Human-readable name.
    pub name: String,
    /// Absolute path to the GGUF file on disk.
    pub file_path: String,
    /// Parameter count in billions.
    pub param_count_b: f64,
    /// Model architecture (e.g. `"llama"`, `"mistral"`).
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub architecture: Option<String>,
    /// Quantization type (e.g. `"Q4_K_M"`, `"F16"`).
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantization: Option<String>,
    /// Maximum context length in tokens.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number", optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_length: Option<u64>,
    // ── MoE topology (omitted for non-MoE models) ─────────────────────────────
    /// Total number of experts (MoE models only).
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expert_count: Option<u32>,
    /// Experts activated per token (MoE models only).
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expert_used_count: Option<u32>,
    /// Shared experts that are always active (MoE models only).
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expert_shared_count: Option<u32>,
    // ── HuggingFace provenance ────────────────────────────────────────────────
    /// HuggingFace repository ID (e.g. `"bartowski/Llama-3.1-8B-GGUF"`).
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hf_repo_id: Option<String>,
    /// Original filename on HuggingFace Hub.
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hf_filename: Option<String>,
    /// Git commit SHA from HuggingFace Hub.
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hf_commit_sha: Option<String>,
    /// When the model was downloaded from HuggingFace (`"%Y-%m-%d %H:%M:%S"`).
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub download_date: Option<String>,
    /// Last time an update check was performed (`"%Y-%m-%d %H:%M:%S"`).
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_update_check: Option<String>,
    // ── Organisation ──────────────────────────────────────────────────────────
    /// User-defined and auto-generated tags.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Capability flags serialized as a `u32` bit-field.
    ///
    /// A `bitflags` newtype, so it crosses the wire as a bare number and
    /// cannot derive `TS` itself — see [`GuiModel::capabilities`].
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    #[serde(default)]
    pub capabilities: gglib_core::ModelCapabilities,
    // ── Inference defaults ────────────────────────────────────────────────────
    /// Per-model inference parameter overrides.
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inference_defaults: Option<gglib_core::domain::InferenceConfig>,
    /// Whether [`Self::inference_defaults`] was set by the user or
    /// auto-detected at import time. See `gglib_core::domain::DefaultsOrigin`.
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defaults_origin: Option<gglib_core::domain::DefaultsOrigin>,
    /// Whether this model's chat template reads `reasoning_effort`, as
    /// llama-server reported it the last time the model was served.
    ///
    /// # Three states, and the third is the common one
    ///
    /// Carried as [`Support`] rather than a `bool` or an `Option<bool>`,
    /// because a client has to be able to tell *not supported* from *nobody has
    /// looked*. Most rows are the latter: the caps are read from `GET /props`
    /// while the model is running, so every model that has never been launched
    /// on this installation answers `unknown` and keeps answering it until it
    /// is.
    ///
    /// A `bool` would collapse `unknown` into `false`, and a client would then
    /// grey out the reasoning control on every unlaunched model — the
    /// unknown-gates mistake ADR 0007 decision 3 forbids the server to make,
    /// reproduced one layer out. The server's own suppression acts only on
    /// [`Support::No`]; a surface should offer the control on `yes` and
    /// `unknown` alike, and explain itself on `no`.
    ///
    /// # Why the one field and not the whole caps object
    ///
    /// `TemplateCaps` carries nine bools. Eight describe things gglib already
    /// models in [`Self::capabilities`] from its own catalog — tools, system
    /// role, parallel calls — and publishing a second, differently-sourced
    /// answer to the same question invites a client to read whichever it finds
    /// first. The ninth has no other home, and this is it. A surface that
    /// genuinely needs the raw self-report should get its own endpoint rather
    /// than a `serde` alias on this one.
    ///
    /// [`Support`]: gglib_core::domain::Support
    #[serde(default)]
    pub reasoning_effort_support: gglib_core::domain::Support,
    // ── Timestamps ────────────────────────────────────────────────────────────
    /// When the model was first added to the database (`"%Y-%m-%d %H:%M:%S"`).
    pub added_at: String,
    // ── Serving status ────────────────────────────────────────────────────────
    /// Whether the model is currently being served.
    #[serde(default)]
    pub is_serving: bool,
    /// Port the model is served on, if currently serving.
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
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
            defaults_origin: model.defaults_origin,
            // The one derivation in this conversion, and it is the tri-state's
            // own: `reasoning_effort_support` answers `Unknown` both for a
            // model with no stored caps and for caps that omitted the field.
            // Neither is a "no", and neither may be rendered as one.
            reasoning_effort_support: gglib_core::domain::reasoning_effort_support(
                &model.template_caps,
            ),
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

/// Request body for updating a model.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
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
    ///
    /// ts-rs cannot read a nested `Option`, so the three states are spelled
    /// out by hand: absent, `null`, or a value.
    // `as` rather than `type`: ts-rs registers a field's dependencies from its
    // Rust type, and a `type = "…"` override replaces the type without
    // registering anything, so the emitted file names `ServerConfig` and never
    // imports it. `as` states a substitute type, which is both rendered and
    // followed for imports. `optional = nullable` then gives `field?: T | null`
    // — the same three states, with the import.
    #[cfg_attr(
        feature = "ts-bindings",
        ts(as = "Option<gglib_core::domain::ServerConfig>", optional = nullable)
    )]
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

/// Whether a newer HuggingFace revision exists for a model.
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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct ModelsDirectoryInfo {
    pub path: String,
    pub source: String,
    pub default_path: String,
    pub exists: bool,
    pub writable: bool,
}

/// Application settings for the settings UI.
///
/// `Default` is "nothing configured" — every field is optional, so it stands
/// for a fresh install with no saved values, which is what callers resolving
/// their own fallbacks need to test against.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub default_download_path: Option<String>,
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null"))]
    pub default_context_size: Option<u64>,
    pub proxy_port: Option<u16>,
    pub llama_base_port: Option<u16>,
    pub max_download_queue_size: Option<u32>,
    pub show_memory_fit_indicators: Option<bool>,
    pub max_tool_iterations: Option<u32>,
    pub max_stagnation_steps: Option<u32>,
    /// Default model ID for quick commands (e.g., `gglib question`).
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null"))]
    pub default_model_id: Option<i64>,
    pub inference_defaults: Option<gglib_core::domain::InferenceConfig>,
    /// Named sampling profiles, selectable per request as `{model}:{profile}`.
    pub inference_profiles: Option<Vec<gglib_core::domain::InferenceProfile>>,
    // Setup wizard
    pub setup_completed: Option<bool>,
    // Title generation
    pub title_generation_prompt: Option<String>,
    // Network binding (see `gglib_core::Settings`)
    pub bind_host: Option<String>,
    pub share_lan: Option<bool>,
    pub proxy_api_key: Option<String>,
    // Sampling authority (see `gglib_core::Settings`)
    pub trust_client_sampling: Option<bool>,
    // Proxy loop guard; `None` means enabled (see `gglib_core::Settings`)
    pub proxy_loop_detection: Option<bool>,
    /// Whether a tool call failing schema validation is re-issued with
    /// `tool_choice: "required"`. Absent means on.
    pub tool_call_repair: Option<bool>,
    /// Whether structured-output turns get their temperature capped when no
    /// human chose one. Absent means on (see `gglib_core::Settings`). Was
    /// write-only until the GUI grew a toggle — a toggle that saves but
    /// cannot read back silently resets on every reopen.
    pub agentic_sampling: Option<bool>,
    // Always-on proxy, desktop app only (see `gglib_core::Settings`)
    pub proxy_autostart: Option<bool>,
    pub close_to_tray: Option<bool>,
    pub start_at_login: Option<bool>,
}

impl From<gglib_core::Settings> for AppSettings {
    fn from(settings: gglib_core::Settings) -> Self {
        Self {
            default_download_path: settings.default_download_path,
            default_context_size: settings.default_context_size,
            proxy_port: settings.proxy_port,
            llama_base_port: settings.llama_base_port,
            max_download_queue_size: settings.max_download_queue_size,
            show_memory_fit_indicators: settings.show_memory_fit_indicators,
            max_tool_iterations: settings.max_tool_iterations,
            max_stagnation_steps: settings.max_stagnation_steps,
            default_model_id: settings.default_model_id,
            inference_defaults: settings.inference_defaults,
            inference_profiles: settings.inference_profiles,
            setup_completed: settings.setup_completed,
            title_generation_prompt: settings.title_generation_prompt,
            bind_host: settings.bind_host,
            share_lan: settings.share_lan,
            proxy_api_key: settings.proxy_api_key,
            trust_client_sampling: settings.trust_client_sampling,
            proxy_loop_detection: settings.proxy_loop_detection,
            tool_call_repair: settings.tool_call_repair,
            agentic_sampling: settings.agentic_sampling,
            proxy_autostart: settings.proxy_autostart,
            close_to_tray: settings.close_to_tray,
            start_at_login: settings.start_at_login,
        }
    }
}

/// Request body for updating application settings.
///
/// Every field is `Option<Option<T>>` with `serde_with::rust::double_option`
/// so an explicit JSON `null` (clear the setting) is distinguished from an
/// omitted key (leave unchanged) — the same pattern used by
/// [`UpdateModelRequest::server_defaults`].
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct UpdateSettingsRequest {
    // Every field below is a `double_option`, which ts-rs cannot read through:
    // the nested `Option` needs an explicit type, spelled `T | null` with
    // `optional` so the emitted `field?: T | null` carries all three states —
    // absent leaves the setting alone, `null` clears it, a value sets it.
    #[cfg_attr(feature = "ts-bindings", ts(type = "string | null", optional))]
    #[serde(default, with = "serde_with::rust::double_option")]
    pub default_download_path: Option<Option<String>>,
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null", optional))]
    #[serde(default, with = "serde_with::rust::double_option")]
    pub default_context_size: Option<Option<u64>>,
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null", optional))]
    #[serde(default, with = "serde_with::rust::double_option")]
    pub proxy_port: Option<Option<u16>>,
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null", optional))]
    #[serde(default, with = "serde_with::rust::double_option")]
    pub llama_base_port: Option<Option<u16>>,
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null", optional))]
    #[serde(default, with = "serde_with::rust::double_option")]
    pub max_download_queue_size: Option<Option<u32>>,
    #[cfg_attr(feature = "ts-bindings", ts(type = "boolean | null", optional))]
    #[serde(default, with = "serde_with::rust::double_option")]
    pub show_memory_fit_indicators: Option<Option<bool>>,
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null", optional))]
    #[serde(default, with = "serde_with::rust::double_option")]
    pub max_tool_iterations: Option<Option<u32>>,
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null", optional))]
    #[serde(default, with = "serde_with::rust::double_option")]
    pub max_stagnation_steps: Option<Option<u32>>,
    /// Default model ID for quick commands (e.g., `gglib question`).
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null", optional))]
    #[serde(default, with = "serde_with::rust::double_option")]
    pub default_model_id: Option<Option<i64>>,
    // `as` rather than `type`, for the import — see `server_defaults` above.
    #[cfg_attr(
        feature = "ts-bindings",
        ts(as = "Option<gglib_core::domain::InferenceConfig>", optional = nullable)
    )]
    #[serde(default, with = "serde_with::rust::double_option")]
    pub inference_defaults: Option<Option<gglib_core::domain::InferenceConfig>>,
    /// Replaces the whole profile list. `null` clears it; an omitted key leaves
    /// it untouched, so a client updating an unrelated setting cannot drop
    /// profiles it never knew about.
    #[cfg_attr(
        feature = "ts-bindings",
        ts(as = "Option<Vec<gglib_core::domain::InferenceProfile>>", optional = nullable)
    )]
    #[serde(default, with = "serde_with::rust::double_option")]
    pub inference_profiles: Option<Option<Vec<gglib_core::domain::InferenceProfile>>>,
    // Setup wizard
    #[cfg_attr(feature = "ts-bindings", ts(type = "boolean | null", optional))]
    #[serde(default, with = "serde_with::rust::double_option")]
    pub setup_completed: Option<Option<bool>>,
    // Title generation
    #[cfg_attr(feature = "ts-bindings", ts(type = "string | null", optional))]
    #[serde(default, with = "serde_with::rust::double_option")]
    pub title_generation_prompt: Option<Option<String>>,
    // Network binding (see `gglib_core::Settings`)
    #[cfg_attr(feature = "ts-bindings", ts(type = "string | null", optional))]
    #[serde(default, with = "serde_with::rust::double_option")]
    pub bind_host: Option<Option<String>>,
    #[cfg_attr(feature = "ts-bindings", ts(type = "boolean | null", optional))]
    #[serde(default, with = "serde_with::rust::double_option")]
    pub share_lan: Option<Option<bool>>,
    #[cfg_attr(feature = "ts-bindings", ts(type = "string | null", optional))]
    #[serde(default, with = "serde_with::rust::double_option")]
    pub proxy_api_key: Option<Option<String>>,
    // Sampling authority (see `gglib_core::Settings`)
    #[cfg_attr(feature = "ts-bindings", ts(type = "boolean | null", optional))]
    #[serde(default, with = "serde_with::rust::double_option")]
    pub trust_client_sampling: Option<Option<bool>>,
    // Proxy loop guard; explicit `null` re-enables (see `gglib_core::Settings`)
    #[cfg_attr(feature = "ts-bindings", ts(type = "boolean | null", optional))]
    #[serde(default, with = "serde_with::rust::double_option")]
    pub proxy_loop_detection: Option<Option<bool>>,
    // Proxy tool-call repair; explicit `null` re-enables (see `gglib_core::Settings`)
    #[cfg_attr(feature = "ts-bindings", ts(type = "boolean | null", optional))]
    #[serde(default, with = "serde_with::rust::double_option")]
    pub tool_call_repair: Option<Option<bool>>,
    // Agentic-turn sampling; explicit `null` re-enables (see `gglib_core::Settings`)
    #[cfg_attr(feature = "ts-bindings", ts(type = "boolean | null", optional))]
    #[serde(
        default,
        alias = "toolCallFloor",
        with = "serde_with::rust::double_option"
    )]
    pub agentic_sampling: Option<Option<bool>>,
    // Always-on proxy, desktop app only (see `gglib_core::Settings`)
    #[cfg_attr(feature = "ts-bindings", ts(type = "boolean | null", optional))]
    #[serde(default, with = "serde_with::rust::double_option")]
    pub proxy_autostart: Option<Option<bool>>,
    #[cfg_attr(feature = "ts-bindings", ts(type = "boolean | null", optional))]
    #[serde(default, with = "serde_with::rust::double_option")]
    pub close_to_tray: Option<Option<bool>>,
    #[cfg_attr(feature = "ts-bindings", ts(type = "boolean | null", optional))]
    #[serde(default, with = "serde_with::rust::double_option")]
    pub start_at_login: Option<Option<bool>>,
}

impl From<UpdateSettingsRequest> for gglib_core::SettingsUpdate {
    fn from(request: UpdateSettingsRequest) -> Self {
        Self {
            default_download_path: request.default_download_path,
            default_context_size: request.default_context_size,
            proxy_port: request.proxy_port,
            llama_base_port: request.llama_base_port,
            max_download_queue_size: request.max_download_queue_size,
            show_memory_fit_indicators: request.show_memory_fit_indicators,
            max_tool_iterations: request.max_tool_iterations,
            max_stagnation_steps: request.max_stagnation_steps,
            default_model_id: request.default_model_id,
            inference_defaults: request.inference_defaults,
            inference_profiles: request.inference_profiles,
            setup_completed: request.setup_completed,
            title_generation_prompt: request.title_generation_prompt,
            bind_host: request.bind_host,
            share_lan: request.share_lan,
            proxy_api_key: request.proxy_api_key,
            trust_client_sampling: request.trust_client_sampling,
            proxy_loop_detection: request.proxy_loop_detection,
            tool_call_repair: request.tool_call_repair,
            agentic_sampling: request.agentic_sampling,
            proxy_autostart: request.proxy_autostart,
            close_to_tray: request.close_to_tray,
            start_at_login: request.start_at_login,
        }
    }
}

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

/// Request to call an MCP tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct McpToolCallRequest {
    pub tool_name: String,
    /// Tool arguments, shaped by the tool's own schema and opaque here.
    #[cfg_attr(feature = "ts-bindings", ts(type = "Record<string, unknown>"))]
    pub arguments: std::collections::HashMap<String, serde_json::Value>,
}

/// Response from an MCP tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct McpToolCallResponse {
    pub success: bool,
    /// Whatever the tool returned. Opaque here, as in [`McpToolInfo`].
    #[cfg_attr(feature = "ts-bindings", ts(type = "unknown"))]
    pub data: Option<serde_json::Value>,
    pub error: Option<String>,
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
