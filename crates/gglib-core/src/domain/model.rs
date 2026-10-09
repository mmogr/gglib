//! Model domain types.
//!
//! These types represent models in the system, independent of any
//! infrastructure concerns (database, filesystem, etc.).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use super::capabilities::ModelCapabilities;
use super::image_family::{ComponentRole, ImageFamily};
use super::inference::{DefaultsOrigin, InferenceConfig};
use super::runtime_kind::RuntimeKind;
use super::server_config::ServerConfig;

// ─────────────────────────────────────────────────────────────────────────────
// System tags
// ─────────────────────────────────────────────────────────────────────────────

/// Prefix marking a tag as runtime-load-bearing.
///
/// Tags with this prefix (e.g. `format:qwen-xml`) drive the universal
/// normalization pipeline's parser selection at compose time. Removing
/// one would silently break dialect handling for the affected model, so
/// the tag-mutation API rejects deletions.
pub(super) const SYSTEM_TAG_PREFIX: &str = "format:";

/// Returns `true` when `tag` is a system tag that callers must not
/// remove through the standard tag-mutation API.
#[must_use]
pub fn is_system_tag(tag: &str) -> bool {
    tag.starts_with(SYSTEM_TAG_PREFIX)
}

// ─────────────────────────────────────────────────────────────────────────────
// Filter/Aggregate Types
// ─────────────────────────────────────────────────────────────────────────────

/// Filter options for the model library UI.
///
/// Contains aggregate data about available models for building
/// dynamic filter controls.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct ModelFilterOptions {
    /// All distinct quantization types present in the library.
    pub quantizations: Vec<String>,
    /// Minimum and maximum parameter counts (in billions).
    pub param_range: Option<RangeValues>,
    /// Minimum and maximum context lengths.
    pub context_range: Option<RangeValues>,
    /// Minimum and maximum `latest_tg_tps` from benchmark summaries.
    /// `None` when no models have been benchmarked.
    pub speed_range: Option<RangeValues>,
}

/// A range of numeric values with min and max.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct RangeValues {
    pub min: f64,
    pub max: f64,
}

// ─────────────────────────────────────────────────────────────────────────────
// Model Types
// ─────────────────────────────────────────────────────────────────────────────

/// A model that exists in the system with a database ID.
///
/// This represents a persisted model with all its metadata.
/// Use `NewModel` for models that haven't been persisted yet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    /// Database ID of the model (always present for persisted models).
    pub id: i64,
    /// Human-readable name for the model.
    pub name: String,
    /// Canonical deduplication key (e.g., `hf:repo@sha#file`).
    #[serde(default)]
    pub model_key: String,
    /// Absolute path to the GGUF file on the filesystem.
    pub file_path: PathBuf,
    /// Absolute path to the projector this model loads beside its weights
    /// (`--mmproj`), which is what gives it image input. Several models may
    /// name one file.
    #[serde(default)]
    pub projector_path: Option<PathBuf>,
    /// Number of parameters in the model (in billions).
    pub param_count_b: f64,
    /// Model architecture (e.g., "llama", "mistral", "falcon").
    pub architecture: Option<String>,
    /// Quantization type (e.g., "`Q4_0`", "`Q8_0`", "`F16`", "`F32`").
    pub quantization: Option<String>,
    /// Maximum context length the model supports.
    pub context_length: Option<u64>,
    /// Number of experts (for `MoE` models).
    pub expert_count: Option<u32>,
    /// Number of experts used during inference (for `MoE` models).
    pub expert_used_count: Option<u32>,
    /// Number of shared experts (for `MoE` models).
    pub expert_shared_count: Option<u32>,
    /// Additional metadata key-value pairs from the GGUF file.
    pub metadata: HashMap<String, String>,
    /// UTC timestamp of when the model was added to the database.
    pub added_at: DateTime<Utc>,
    /// `HuggingFace` repository ID (e.g., "`TheBloke/Llama-2-7B-GGUF`").
    pub hf_repo_id: Option<String>,
    /// Git commit SHA from `HuggingFace` Hub.
    pub hf_commit_sha: Option<String>,
    /// Original filename on `HuggingFace` Hub.
    pub hf_filename: Option<String>,
    /// Timestamp of when this model was downloaded from `HuggingFace`.
    pub download_date: Option<DateTime<Utc>>,
    /// Last time we checked for updates on `HuggingFace`.
    pub last_update_check: Option<DateTime<Utc>>,
    /// User-defined tags for organizing models.
    pub tags: Vec<String>,
    /// Model capabilities inferred from chat template analysis.
    #[serde(default)]
    pub capabilities: ModelCapabilities,
    /// Per-model inference parameter defaults.
    ///
    /// These are preferred over global settings when making inference requests.
    /// If not set, falls back to global settings or hardcoded defaults.
    #[serde(default)]
    pub inference_defaults: Option<InferenceConfig>,
    /// Whether [`Self::inference_defaults`] was set by the user or
    /// auto-detected at import time from the `reasoning` tag.
    ///
    /// Always `None` when [`Self::inference_defaults`] is `None` — there is
    /// nothing to have an origin. See [`DefaultsOrigin`] for why this
    /// changes how resolution ranks the field.
    #[serde(default)]
    pub defaults_origin: Option<DefaultsOrigin>,
    /// Per-model server-level defaults (`context_length`, etc.).
    ///
    /// Stored as JSON in the database. Overrides global settings but can
    /// be overridden at request time. Part of the 5-level fallback chain.
    #[serde(default)]
    pub server_defaults: Option<ServerConfig>,
    /// Tool-call dialect spec detected at import/retag time.
    ///
    /// Stored as JSON in the database. `None` for rows imported before
    /// specs existed and for models whose dialect could not be derived —
    /// consumers fall back to the `format:*` tag mapping.
    #[serde(default)]
    pub dialect_spec: Option<crate::domain::dialect::DialectSpec>,
    /// llama-server's template-capability self-report (`chat_template_caps`
    /// from `GET /props`), recorded once a launch has observed it.
    ///
    /// Stored as JSON in the database. `None` means **never observed** — the
    /// third state of ADR 0007's tri-state, never to be collapsed into "not
    /// supported". Unlike [`Self::dialect_spec`] this is not derived at
    /// import time: it is a fact about the binary–model pair, so only a
    /// launch can learn it.
    #[serde(default)]
    pub template_caps: Option<crate::domain::TemplateCaps>,
    /// Denormalised benchmark summary joined from `model_benchmark_summaries`.
    ///
    /// `None` when no benchmark has been run for this model yet, or when the
    /// model is fetched without the summary join (e.g. lightweight lookups).
    #[serde(default)]
    pub benchmark_summary: Option<crate::domain::benchmark::ModelBenchmarkSummary>,
    /// The image family this model draws as, read from its tensor names at
    /// import or retag; `None` for a model that chats.
    #[serde(default)]
    pub image_family: Option<ImageFamily>,
    /// The files this image model draws with beside its weights, one per
    /// role at most. Several models may name one file.
    #[serde(default)]
    pub components: Vec<ModelComponent>,
}

/// A model to be inserted into the system (no ID yet).
///
/// This represents a model that hasn't been persisted to the database.
/// After insertion, the repository returns a `Model` with the assigned ID.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewModel {
    /// Human-readable name for the model.
    pub name: String,
    /// Absolute path to the GGUF file on the filesystem.
    pub file_path: PathBuf,
    /// Absolute path to the projector this model loads beside its weights
    /// (`--mmproj`), which is what gives it image input. Several models may
    /// name one file.
    #[serde(default)]
    pub projector_path: Option<PathBuf>,
    /// Number of parameters in the model (in billions).
    pub param_count_b: f64,
    /// Model architecture (e.g., "llama", "mistral", "falcon").
    pub architecture: Option<String>,
    /// Quantization type (e.g., "`Q4_0`", "`Q8_0`", "`F16`", "`F32`").
    pub quantization: Option<String>,
    /// Maximum context length the model supports.
    pub context_length: Option<u64>,
    /// Number of experts (for `MoE` models).
    pub expert_count: Option<u32>,
    /// Number of experts used during inference (for `MoE` models).
    pub expert_used_count: Option<u32>,
    /// Number of shared experts (for `MoE` models).
    pub expert_shared_count: Option<u32>,
    /// Additional metadata key-value pairs from the GGUF file.
    pub metadata: HashMap<String, String>,
    /// UTC timestamp of when the model was added to the database.
    pub added_at: DateTime<Utc>,
    /// `HuggingFace` repository ID (e.g., "`TheBloke/Llama-2-7B-GGUF`").
    pub hf_repo_id: Option<String>,
    /// Git commit SHA from `HuggingFace` Hub.
    pub hf_commit_sha: Option<String>,
    /// Original filename on `HuggingFace` Hub.
    pub hf_filename: Option<String>,
    /// Timestamp of when this model was downloaded from `HuggingFace`.
    pub download_date: Option<DateTime<Utc>>,
    /// Last time we checked for updates on `HuggingFace`.
    pub last_update_check: Option<DateTime<Utc>>,
    /// User-defined tags for organizing models.
    pub tags: Vec<String>,
    /// Ordered list of all file paths for sharded models (None for single-file models).
    pub file_paths: Option<Vec<PathBuf>>,
    /// Model capabilities inferred from chat template analysis.
    #[serde(default)]
    pub capabilities: ModelCapabilities,
    /// Per-model inference parameter defaults.
    ///
    /// These are preferred over global settings when making inference requests.
    /// If not set, falls back to global settings or hardcoded defaults.
    #[serde(default)]
    pub inference_defaults: Option<InferenceConfig>,
    /// See [`Model::defaults_origin`].
    #[serde(default)]
    pub defaults_origin: Option<DefaultsOrigin>,
    /// Per-model server startup defaults.
    #[serde(default)]
    pub server_defaults: Option<ServerConfig>,
    /// See [`Model::dialect_spec`].
    #[serde(default)]
    pub dialect_spec: Option<crate::domain::dialect::DialectSpec>,
    /// See [`Model::image_family`].
    #[serde(default)]
    pub image_family: Option<ImageFamily>,
    /// See [`Model::components`].
    #[serde(default)]
    pub components: Vec<ModelComponent>,
}

/// A file an image model draws with beside its weights, in the role it plays.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelComponent {
    /// The role the file plays.
    pub role: ComponentRole,
    /// Absolute path to the file.
    pub path: PathBuf,
}

impl Model {
    /// Whether this model reads images: it does exactly when it has a
    /// projector.
    #[must_use]
    pub const fn image_input(&self) -> bool {
        self.projector_path.is_some()
    }

    /// Whether this model draws images: it does exactly when its tensors
    /// named an image family.
    #[must_use]
    pub const fn generates_images(&self) -> bool {
        self.image_family.is_some()
    }

    /// The program that serves this model: stable-diffusion.cpp when it
    /// draws images, llama.cpp otherwise.
    #[must_use]
    pub const fn runtime(&self) -> RuntimeKind {
        RuntimeKind::of(self.image_family)
    }

    /// The roles this model's family needs that it has no file linked for,
    /// in the recipe's order; empty for a model that chats.
    #[must_use]
    pub fn missing_components(&self) -> Vec<ComponentRole> {
        let Some(family) = self.image_family else {
            return Vec::new();
        };
        family
            .recipe()
            .components
            .iter()
            .map(|spec| spec.role)
            .filter(|role| !self.components.iter().any(|c| c.role == *role))
            .collect()
    }
}

impl NewModel {
    /// Create a new model with minimal required fields.
    ///
    /// Other fields are set to `None` or empty defaults.
    #[must_use]
    pub fn new(
        name: String,
        file_path: PathBuf,
        param_count_b: f64,
        added_at: DateTime<Utc>,
    ) -> Self {
        Self {
            name,
            file_path,
            projector_path: None,
            param_count_b,
            architecture: None,
            quantization: None,
            context_length: None,
            expert_count: None,
            expert_used_count: None,
            expert_shared_count: None,
            metadata: HashMap::new(),
            added_at,
            hf_repo_id: None,
            hf_commit_sha: None,
            hf_filename: None,
            download_date: None,
            last_update_check: None,
            tags: Vec::new(),
            file_paths: None,
            capabilities: ModelCapabilities::default(),
            inference_defaults: None,
            defaults_origin: None,
            server_defaults: None,
            dialect_spec: None,
            image_family: None,
            components: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_new_model_creation() {
        let model = NewModel::new(
            "Test Model".to_string(),
            PathBuf::from("/path/to/model.gguf"),
            7.0,
            Utc::now(),
        );

        assert_eq!(model.name, "Test Model");
        assert!((model.param_count_b - 7.0).abs() < f64::EPSILON);
        assert!(model.architecture.is_none());
        assert!(model.tags.is_empty());
    }

    fn flux(components: &[ComponentRole]) -> Model {
        let mut new = NewModel::new(
            "flux".to_owned(),
            PathBuf::from("/models/flux1-schnell-q8_0.gguf"),
            12.0,
            Utc::now(),
        );
        new.image_family = Some(ImageFamily::Flux1);
        new.components = components
            .iter()
            .map(|role| ModelComponent {
                role: *role,
                path: PathBuf::from(format!("/models/{role}.safetensors")),
            })
            .collect();
        Model::stored(1, &new)
    }

    #[test]
    fn a_model_that_draws_is_served_by_stable_diffusion_and_one_that_chats_by_llama() {
        assert_eq!(flux(&[]).runtime(), RuntimeKind::StableDiffusion);
        let mut chat = flux(&[]);
        chat.image_family = None;
        assert_eq!(chat.runtime(), RuntimeKind::Llama);
    }

    #[test]
    fn missing_components_are_the_recipe_roles_with_no_link() {
        assert_eq!(
            flux(&[]).missing_components(),
            [
                ComponentRole::Vae,
                ComponentRole::ClipL,
                ComponentRole::T5xxl
            ]
        );
        assert_eq!(
            flux(&[ComponentRole::ClipL]).missing_components(),
            [ComponentRole::Vae, ComponentRole::T5xxl]
        );
        let all = [
            ComponentRole::T5xxl,
            ComponentRole::Vae,
            ComponentRole::ClipL,
        ];
        assert!(flux(&all).missing_components().is_empty());
    }

    #[test]
    fn a_chat_model_misses_no_component() {
        let model = Model::stored(
            1,
            &NewModel::new("qwen".to_owned(), PathBuf::from("/m.gguf"), 7.0, Utc::now()),
        );
        assert!(model.missing_components().is_empty());
    }
}
