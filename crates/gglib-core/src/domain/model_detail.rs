//! [`ModelDetailDto`]: every stored field of one model, as the inspector
//! reads it; and [`ModelLookup`], one model as a paired machine reads it.

use serde::{Deserialize, Serialize};

use super::Model;
// Named as the adapters name it: the doc comments below are copied verbatim
// into the generated `ModelDetailDto.ts`, and their links say `gglib_core::`.
use crate as gglib_core;

/// Complete model details for the inspect view.
///
/// Carries the domain [`Model`] in full — raw GGUF metadata, `MoE` topology and
/// `HuggingFace` provenance, none of which the list endpoint sends. It is the
/// single shared contract consumed by:
///
/// - CLI: `gglib model inspect` (human-readable or `--json`)
/// - Axum: `GET /api/models/:id/detail`
/// - GUI frontend: model detail panel
/// - Proxy: `GET /v1/models/{name}/detail`, inside a [`ModelLookup`], with
///   [`Self::file_path`] and [`Self::port`] left out
///
/// # Not a superset of `GuiModel`
///
/// The two shapes overlap; neither contains the other. A TypeScript mirror
/// that extended the list row would advertise `server_defaults` and
/// `benchmark_summary` on a response that carries neither.
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
    /// Absolute path to the GGUF file on disk. `None` where the reader is on
    /// another machine, to which this machine's file layout means nothing.
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    /// Absolute path to the projector the model loads beside its weights.
    /// `None` when it has none, and where the reader is on another machine,
    /// as `file_path` is; [`Self::image_input`] is the answer that travels.
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub projector_path: Option<String>,
    /// Whether the model reads images: it is linked to a projector.
    #[serde(default)]
    pub image_input: bool,
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
    // ── HuggingFace provenance ────────────────────────────────────────────────
    /// `HuggingFace` repository ID (e.g. `"bartowski/Llama-3.1-8B-GGUF"`).
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hf_repo_id: Option<String>,
    /// Original filename on `HuggingFace` Hub.
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hf_filename: Option<String>,
    /// Git commit SHA from `HuggingFace` Hub.
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hf_commit_sha: Option<String>,
    /// When the model was downloaded from `HuggingFace` (`"%Y-%m-%d %H:%M:%S"`).
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
    /// cannot derive `TS` itself — see `GuiModel::capabilities`.
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
    /// [`Support::No`]: gglib_core::domain::Support::No
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
        let image_input = model.image_input();
        Self {
            id: model.id,
            name: model.name,
            file_path: Some(model.file_path.to_string_lossy().to_string()),
            image_input,
            projector_path: model
                .projector_path
                .map(|path| path.to_string_lossy().to_string()),
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

/// One model as `GET /v1/models/{name}/detail` answers it: what the
/// identifier resolved to, and the profile it named.
///
/// The identifier is resolved as a chat request's is — catalogue id first,
/// then exact name, and a `:profile` suffix routed to a configured profile —
/// so a caller that reads this before a turn sends the turn to the same model.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ModelLookup {
    /// The inference profile the identifier named, as `name:profile` does.
    /// `None` for an identifier that named the model alone.
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    /// The model, without its file path or port.
    pub detail: ModelDetailDto,
}
