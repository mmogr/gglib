//! Model catalog port for listing and resolving models.
//!
//! This port defines the interface for querying the model catalog.
//! It provides domain-level model information without exposing
//! database or storage implementation details.

use async_trait::async_trait;
use std::fmt;
use std::path::PathBuf;
use thiserror::Error;

use crate::domain::DefaultsOrigin;
use crate::domain::DialectSpec;
use crate::domain::InferenceConfig;
use crate::domain::KvElemsPerToken;
use crate::domain::ModelCapabilities;
use crate::domain::ModelSamplingDefaults;
use crate::domain::ServerConfig;
use crate::domain::TemplateCaps;

/// Domain model summary for catalog operations (listing).
///
/// This is a domain type (not an `OpenAI` API type). The proxy layer
/// is responsible for mapping this to OpenAI-compatible formats.
///
/// Note: Does NOT include `file_path` to avoid leaking filesystem details
/// in catalog/listing operations.
#[derive(Debug, Clone)]
pub struct ModelSummary {
    /// Database ID of the model.
    pub id: u32,
    /// Model name (used as identifier).
    pub name: String,
    /// Tags/labels associated with the model.
    pub tags: Vec<String>,
    /// Detected and persisted capability flags for this model.
    ///
    /// This is the single source of truth for model behaviour constraints
    /// (strict-turn alternation, system-role support, tool calls, reasoning).
    /// The proxy uses these directly rather than inferring from tags at
    /// request time, eliminating the split-brain between tags and capabilities.
    pub capabilities: ModelCapabilities,
    /// Parameter count as string (e.g., "7B", "13B", "70B").
    pub param_count: String,
    /// Quantization type (e.g., "`Q4_K_M`", "`Q8_0`").
    pub quantization: Option<String>,
    /// Model architecture (e.g., "llama", "mistral", "qwen2").
    pub architecture: Option<String>,
    /// Unix timestamp when the model was added.
    pub created_at: i64,
    /// File size in bytes.
    pub file_size: u64,
    /// Maximum context length the model supports, in tokens (from GGUF
    /// metadata). `None` when unknown.
    ///
    /// This is a static, per-model ceiling — it does not reflect the
    /// `--ctx-size` a currently-running instance was actually launched
    /// with, which can be smaller (see `gglib_core::ports::model_runtime::RunningTarget::effective_ctx`
    /// for the live value). Consumers that need the true, currently-running
    /// context size should prefer `effective_ctx` when the model is running
    /// and fall back to this field otherwise.
    pub context_length: Option<u64>,
    /// Per-model inference parameter defaults.
    ///
    /// When `Some`, these are resolved per-request via
    /// [`InferenceConfig::resolve_with_defaults`] before forwarding to llama-server.
    /// Used by `gglib proxy` to inject resolved defaults into OpenAI-format
    /// request bodies, and by the agentic loop (`gglib chat`, `gglib q`) to
    /// apply model-specific sampling parameters.
    pub inference_defaults: Option<InferenceConfig>,
    /// Whether [`Self::inference_defaults`] was set by the user or
    /// auto-detected. See [`DefaultsOrigin`] and
    /// [`InferenceConfig::resolve_with_profile`](crate::domain::InferenceConfig::resolve_with_profile).
    pub defaults_origin: Option<DefaultsOrigin>,
    /// Per-model server defaults (`context_length`, etc.) from the database.
    pub server_defaults: Option<ServerConfig>,
    /// Persisted tool-call dialect spec, when detection identified one.
    ///
    /// `None` for rows imported before specs existed and for models whose
    /// dialect could not be derived — consumers fall back to mapping
    /// `format:*` tags via `normalize::registry::dialect_for_tags`.
    pub dialect: Option<DialectSpec>,
    /// llama-server's template-capability self-report, when a launch has
    /// recorded one (see [`Model::template_caps`](crate::domain::Model)).
    ///
    /// `None` is "never observed", never "unsupported" — ADR 0007's
    /// tri-state, carried whole so no consumer has to reconstruct it.
    pub template_caps: Option<TemplateCaps>,
}

/// Launch specification for running a model.
///
/// Contains all information needed to actually launch a model,
/// including the file path. Separate from `ModelSummary` to avoid
/// leaking filesystem details in catalog operations.
#[derive(Debug, Clone)]
pub struct ModelLaunchSpec {
    /// Database ID of the model.
    pub id: u32,
    /// Model name.
    pub name: String,
    /// Absolute path to the GGUF file.
    pub file_path: PathBuf,
    /// Tags/labels associated with the model.
    pub tags: Vec<String>,
    /// Model architecture (for runtime configuration).
    pub architecture: Option<String>,
    /// Quantization label (`Q4_K_M`), when the catalog recorded one.
    ///
    /// Carried purely so the launch can name what it loaded — see
    /// [`crate::domain::LaunchNarration`]. `None` for models whose GGUF
    /// metadata did not identify a quantization.
    pub quantization: Option<String>,
    /// Maximum context length the model supports.
    pub context_length: Option<u64>,
    /// Per-model server defaults (e.g., `context_length` for launch).
    pub server_defaults: Option<ServerConfig>,
    /// Total on-disk size of the model weights in bytes, summed across all
    /// shards for multi-part GGUFs.
    ///
    /// Used to budget host memory at launch (see
    /// [`crate::server_config::compute_auto_cache_ram_mb`]). `0` when the
    /// size could not be determined — callers must treat that as "unknown"
    /// rather than "free".
    pub file_size_bytes: u64,
    /// Estimated K/V element counts consumed per token of context, derived
    /// from the model's GGUF metadata (see
    /// [`crate::domain::estimate_kv_elems_per_token`]). Type-agnostic —
    /// callers convert to bytes via [`crate::domain::kv_bytes_per_token`]
    /// once the launch's resolved K/V cache types are known.
    ///
    /// `None` when the metadata doesn't carry the layer/head counts needed;
    /// callers substitute a conservative allowance.
    pub kv_elems_per_token: Option<KvElemsPerToken>,
    /// True when the model's KV memory retains only part of the token history
    /// — sliding-window, hybrid, or recurrent attention (see
    /// [`crate::domain::kv_memory_is_partial`]).
    ///
    /// Such models cannot be resumed from llama-server's disk slot files: the
    /// save/restore path does not carry the context checkpoints they need, so
    /// a "successful" restore still forces a full prompt re-prefill. Callers
    /// disable the disk slot layer for these models and rely on the in-RAM
    /// prompt cache, which does preserve checkpoints.
    pub kv_memory_is_partial: bool,
    /// What this model's own GGUF declares about sampler defaults (see
    /// [`crate::domain::ModelSamplingDefaults`]).
    ///
    /// llama.cpp applies these over its own build defaults for every field no
    /// CLI flag sets, and reports the result as `/props`'s
    /// `default_generation_settings`. Carried to the running target so the
    /// proxy's baseline check can tell a model's own recommendation from a pin
    /// bump, rather than reporting the first as the second.
    ///
    /// Derived from the metadata already on the catalog row, the same way
    /// `kv_elems_per_token` and `kv_memory_is_partial` are.
    pub model_sampling: ModelSamplingDefaults,
}

impl ModelSummary {
    /// Create a description string for this model.
    #[must_use]
    pub fn description(&self) -> String {
        let arch = self.architecture.as_deref().unwrap_or("unknown");
        let quant = self.quantization.as_deref().unwrap_or("unknown");
        format!("{} - {} parameters, {}", arch, self.param_count, quant)
    }
}

/// Errors that can occur during catalog operations.
#[derive(Debug, Error)]
pub enum CatalogError {
    /// Failed to query the catalog.
    #[error("Failed to query catalog: {0}")]
    QueryFailed(String),

    /// Internal error during catalog operations.
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Port for querying the model catalog.
///
/// This interface provides read-only access to the model catalog
/// for listing and resolving models. It does not handle model
/// registration or deletion.
#[async_trait]
pub trait ModelCatalogPort: Send + Sync + fmt::Debug {
    /// List all models in the catalog.
    ///
    /// Returns a list of model summaries ordered by name.
    ///
    /// # Errors
    ///
    /// Returns `CatalogError` if the catalog cannot be queried.
    async fn list_models(&self) -> Result<Vec<ModelSummary>, CatalogError>;

    /// Resolve a model by name or alias.
    ///
    /// This method performs model resolution:
    /// 1. Exact name match
    /// 2. Case-insensitive name match
    /// 3. Fuzzy/partial match (implementation-defined)
    ///
    /// Returns `None` if no matching model is found.
    ///
    /// # Arguments
    ///
    /// * `name` - Model name or alias to resolve
    ///
    /// # Errors
    ///
    /// Returns `CatalogError` if the catalog cannot be queried.
    async fn resolve_model(&self, name: &str) -> Result<Option<ModelSummary>, CatalogError>;

    /// Resolve a model for launching.
    ///
    /// Returns full launch specification including file path.
    /// Use this when you need to actually run a model, not just list it.
    ///
    /// # Arguments
    ///
    /// * `name` - Model name or alias to resolve
    ///
    /// # Errors
    ///
    /// Returns `CatalogError` if the catalog cannot be queried.
    async fn resolve_for_launch(&self, name: &str)
    -> Result<Option<ModelLaunchSpec>, CatalogError>;

    /// Record llama-server's template-capability self-report for model `id`.
    ///
    /// Called once per fresh launch, after the just-spawned server's
    /// `GET /props` has been read (ADR 0007: the caps are a fact about the
    /// binary–model pair, so only a launch can learn them). Implementations
    /// persist only when the stored value differs, so repeat launches of an
    /// unchanged pair write nothing.
    ///
    /// Defaulted to a no-op because most implementors of this port are
    /// read-only views (test doubles, the profiles catalog) with nothing to
    /// persist into; only the real database-backed catalog overrides it.
    ///
    /// # Errors
    ///
    /// Returns `CatalogError` if the catalog could not be updated.
    async fn record_template_caps(
        &self,
        _id: u32,
        _caps: TemplateCaps,
    ) -> Result<(), CatalogError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A read-only implementor that never overrides `record_template_caps` —
    /// the shape of every test double this port has across the workspace.
    #[derive(Debug)]
    struct ReadOnlyCatalog;

    #[async_trait]
    impl ModelCatalogPort for ReadOnlyCatalog {
        async fn list_models(&self) -> Result<Vec<ModelSummary>, CatalogError> {
            Ok(Vec::new())
        }
        async fn resolve_model(&self, _name: &str) -> Result<Option<ModelSummary>, CatalogError> {
            Ok(None)
        }
        async fn resolve_for_launch(
            &self,
            _name: &str,
        ) -> Result<Option<ModelLaunchSpec>, CatalogError> {
            Ok(None)
        }
    }

    /// The default body is a successful no-op, so read-only implementors need
    /// not implement persistence they do not have — and a caps observation
    /// against one is dropped, never an error that could fail a launch.
    #[tokio::test]
    async fn record_template_caps_defaults_to_a_successful_no_op() {
        let result = ReadOnlyCatalog
            .record_template_caps(1, TemplateCaps::default())
            .await;
        assert!(result.is_ok());
    }
}
