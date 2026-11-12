//! Model domain types.
//!
//! These types represent models in the system, independent of any
//! infrastructure concerns (database, filesystem, etc.).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use super::capabilities::ModelCapabilities;
use super::file_type::FileType;
use super::image_family::{ComponentRole, ImageFamily};
use super::inference::{DefaultsOrigin, InferenceConfig};
use super::runtime_kind::RuntimeKind;
use super::server_config::ServerConfig;
use super::shard_name::gguf_stem;

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
    /// The parameters, in billions, when they are known: the tensors'
    /// count (`HeaderFacts::param_count_b`), or one its owner set. `None`
    /// when neither the tensors, the labels nor the name gave one.
    #[serde(default)]
    pub param_count_b: Option<f64>,
    /// Model architecture (e.g., "llama", "mistral", "falcon").
    pub architecture: Option<String>,
    /// The name the uploader gave this variant (`UD-Q4_K_XL`), when it came
    /// from a download, or the one its owner set; `None` for a local import
    /// and for a model added before labels were kept.
    #[serde(default)]
    pub variant_label: Option<String>,
    /// The type the header says the file was quantized to, when it says.
    #[serde(default)]
    pub file_type: Option<FileType>,
    /// The bits each weight takes on average, over every shard, read from
    /// the tensor data.
    #[serde(default)]
    pub bits_per_weight: Option<f64>,
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
    /// If not set, falls back to global settings and the model's recipe.
    #[serde(default)]
    pub inference_defaults: Option<InferenceConfig>,
    /// Who wrote [`Self::inference_defaults`]: a person, the import, or a
    /// tune sweep.
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
    /// llama-server's template-capability self-report (`chat_template_caps`
    /// from `GET /props`), recorded once a launch has observed it.
    ///
    /// Stored as JSON in the database. `None` means **never observed** — the
    /// third state of ADR 0007's tri-state, never to be collapsed into "not
    /// supported". It is not derived at import time: it is a fact about the
    /// binary–model pair, so only a launch can learn it.
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
    /// The parameters, in billions, when they are known: the tensors'
    /// count (`HeaderFacts::param_count_b`), or one its owner set. `None`
    /// when neither the tensors, the labels nor the name gave one.
    #[serde(default)]
    pub param_count_b: Option<f64>,
    /// Model architecture (e.g., "llama", "mistral", "falcon").
    pub architecture: Option<String>,
    /// The name the uploader gave this variant (`UD-Q4_K_XL`), when it came
    /// from a download, or the one its owner set; `None` for a local import
    /// and for a model added before labels were kept.
    #[serde(default)]
    pub variant_label: Option<String>,
    /// The type the header says the file was quantized to, when it says.
    #[serde(default)]
    pub file_type: Option<FileType>,
    /// The bits each weight takes on average, over every shard, read from
    /// the tensor data.
    #[serde(default)]
    pub bits_per_weight: Option<f64>,
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
    /// If not set, falls back to global settings and the model's recipe.
    #[serde(default)]
    pub inference_defaults: Option<InferenceConfig>,
    /// See [`Model::defaults_origin`].
    #[serde(default)]
    pub defaults_origin: Option<DefaultsOrigin>,
    /// Per-model server startup defaults.
    #[serde(default)]
    pub server_defaults: Option<ServerConfig>,
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

/// Why a model downloaded before its label was kept cannot be looked up on
/// the Hub again, when no variant of its repository holds its file either.
///
/// Its label is the only name for the files it came as: its header's type
/// does not tell a `UD-Q4_K_XL` download from a `Q4_K_M` one.
pub const NO_HUB_LABEL: &str = "this model was added before gglib kept the name of the \
    quantization it was downloaded as, so its files cannot be found on the Hub again; \
    download it again to check, repair or upgrade it";

impl Model {
    /// The label this model's type is shown and named by: the uploader's
    /// variant label, else the name of the type its header states. An
    /// unknown type number has no name, so no label.
    ///
    /// A variant label that is only the file's own name says nothing of its
    /// type, and the header's type is shown instead: a repository with one
    /// file labels it by its whole name (`z_image-Q8_0`), as it has no
    /// other name to cut a shared prefix from.
    ///
    /// For showing and naming only: a lookup on the Hub reads
    /// [`Self::variant_label`] alone (see [`NO_HUB_LABEL`]).
    #[must_use]
    pub fn quant_label(&self) -> Option<&str> {
        self.variant_label
            .as_deref()
            .filter(|label| !self.is_named(label))
            .or_else(|| self.file_type.and_then(FileType::name))
    }

    /// Whether `label` is this model's file name, less the extension and a
    /// shard's numbering, ignoring case.
    fn is_named(&self, label: &str) -> bool {
        self.file_path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| gguf_stem(name).eq_ignore_ascii_case(label))
    }

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
        missing_roles(self.image_family, &self.components)
    }
}

/// The roles `family`'s recipe needs that `components` has no file for, in
/// the recipe's order; empty for no family. Shared by [`Model`] and the
/// launch spec, so the catalogue and the launch cannot disagree.
#[must_use]
pub(crate) fn missing_roles(
    family: Option<ImageFamily>,
    components: &[ModelComponent],
) -> Vec<ComponentRole> {
    let Some(family) = family else {
        return Vec::new();
    };
    family
        .recipe()
        .components
        .iter()
        .map(|spec| spec.role)
        .filter(|role| !components.iter().any(|c| c.role == *role))
        .collect()
}

impl NewModel {
    /// Create a new model with minimal required fields.
    ///
    /// Other fields are set to `None` or empty defaults.
    #[must_use]
    pub fn new(
        name: String,
        file_path: PathBuf,
        param_count_b: Option<f64>,
        added_at: DateTime<Utc>,
    ) -> Self {
        Self {
            name,
            file_path,
            projector_path: None,
            param_count_b,
            architecture: None,
            variant_label: None,
            file_type: None,
            bits_per_weight: None,
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
            Some(7.0),
            Utc::now(),
        );

        assert_eq!(model.name, "Test Model");
        assert_eq!(model.param_count_b, Some(7.0));
        assert!(model.architecture.is_none());
        assert!(model.tags.is_empty());
    }

    fn flux(components: &[ComponentRole]) -> Model {
        let mut new = NewModel::new(
            "flux".to_owned(),
            PathBuf::from("/models/flux1-schnell-q8_0.gguf"),
            Some(12.0),
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
    fn the_label_is_the_uploaders_else_the_headers_type() {
        let mut model = flux(&[]);
        assert_eq!(model.quant_label(), None);
        model.file_type = Some(FileType(15));
        assert_eq!(model.quant_label(), Some("Q4_K_M"));
        model.variant_label = Some("UD-Q4_K_XL".to_owned());
        assert_eq!(model.quant_label(), Some("UD-Q4_K_XL"));
        model.variant_label = None;
        model.file_type = Some(FileType(42));
        assert_eq!(model.quant_label(), None, "an unknown number has no name");
    }

    fn labelled(path: &str, label: &str) -> Model {
        let mut model = flux(&[]);
        model.file_path = PathBuf::from(path);
        model.variant_label = Some(label.to_owned());
        model.file_type = Some(FileType(7));
        model
    }

    #[test]
    fn a_label_that_is_only_the_files_name_shows_the_headers_type() {
        let one_file = labelled("/m/z_image_nsfw_v2-Q8_0.gguf", "z_image_nsfw_v2-Q8_0");
        assert_eq!(one_file.quant_label(), Some("Q8_0"));
        let any_case = labelled("/m/Z_Image-Q8_0.GGUF", "z_image-q8_0");
        assert_eq!(any_case.quant_label(), Some("Q8_0"));
        let shard = labelled("/m/big-Q8_0-00001-of-00002.gguf", "big-Q8_0");
        assert_eq!(
            shard.quant_label(),
            Some("Q8_0"),
            "a shard goes by its split's name"
        );
    }

    #[test]
    fn a_label_other_than_the_files_name_is_kept() {
        let cut = labelled("/m/Qwen3.8-27B-UD-Q8_K_XL.gguf", "UD-Q8_K_XL");
        assert_eq!(cut.quant_label(), Some("UD-Q8_K_XL"));
        let folder = labelled(
            "/m/UD-IQ3_XXS/Next-UD-IQ3_XXS-00001-of-00004.gguf",
            "UD-IQ3_XXS",
        );
        assert_eq!(folder.quant_label(), Some("UD-IQ3_XXS"));
    }

    #[test]
    fn only_the_files_name_is_compared_never_its_folder() {
        // A label taken from a folder that happens to share the file's name
        // is that name, and says no more of the type than it does.
        let same = labelled("/m/Q8_0-x/Q8_0-x.gguf", "Q8_0-x");
        assert_eq!(same.quant_label(), Some("Q8_0"));
        // A label naming the folder alone is no name of the file's.
        let folder = labelled("/m/z_image-Q8_0/weights.gguf", "z_image-Q8_0");
        assert_eq!(folder.quant_label(), Some("z_image-Q8_0"));
    }

    #[test]
    fn a_files_own_name_with_no_header_type_shows_no_label() {
        let mut model = labelled("/m/z_image-Q8_0.gguf", "z_image-Q8_0");
        model.file_type = None;
        assert_eq!(model.quant_label(), None);
    }

    #[test]
    fn a_chat_model_misses_no_component() {
        let model = Model::stored(
            1,
            &NewModel::new(
                "qwen".to_owned(),
                PathBuf::from("/m.gguf"),
                Some(7.0),
                Utc::now(),
            ),
        );
        assert!(model.missing_components().is_empty());
    }
}
