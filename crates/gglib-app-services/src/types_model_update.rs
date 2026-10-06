//! The body of a model update, and the one place its fields are written
//! onto a model's row.
//!
//! The inspector sends it as `PUT /api/models/{id}` and `gglib model update`
//! builds it from its flags; both hand it to `ModelOps::update`, so the same
//! edit leaves the same row whichever surface made it.

use std::collections::HashMap;
use std::path::PathBuf;

use gglib_core::domain::{DefaultsOrigin, InferenceConfig, Model, ServerConfig};
use serde::{Deserialize, Serialize};

/// Request body for updating a model. A field that is absent leaves its part
/// of the row alone.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct UpdateModelRequest {
    pub name: Option<String>,
    pub quantization: Option<String>,
    pub file_path: Option<String>,
    /// Parameter count, in billions.
    pub param_count_b: Option<f64>,
    pub architecture: Option<String>,
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null"))]
    pub context_length: Option<u64>,
    /// The model's metadata, whole: the map it holds afterwards.
    pub metadata: Option<HashMap<String, String>>,
    /// The model's own sampling defaults, whole, as a person set them. An
    /// empty config clears them, and the model inherits again.
    pub inference_defaults: Option<InferenceConfig>,
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
        ts(as = "Option<ServerConfig>", optional = nullable)
    )]
    #[serde(default, with = "serde_with::rust::double_option")]
    pub server_defaults: Option<Option<ServerConfig>>,
    /// The projector the model loads beside its weights, by path.
    /// - Some(Some(path)) — link the model to the projector at `path`
    /// - Some(None) — unlink it
    /// - None — don't touch the link (key omitted from payload)
    #[cfg_attr(feature = "ts-bindings", ts(as = "Option<String>", optional = nullable))]
    #[serde(default, with = "serde_with::rust::double_option")]
    pub projector_path: Option<Option<String>>,
}

impl UpdateModelRequest {
    /// Write this request onto `model`: each field it carries, and no other.
    ///
    /// `projector_path` is not written here. What may be linked is decided
    /// against the file itself, by `ModelOps::link_projector`, before the
    /// row is read.
    pub fn apply_to(&self, model: &mut Model) {
        if let Some(name) = &self.name {
            model.name.clone_from(name);
        }
        if let Some(quantization) = &self.quantization {
            model.quantization = Some(quantization.clone());
        }
        if let Some(file_path) = &self.file_path {
            model.file_path = PathBuf::from(file_path);
        }
        if let Some(param_count_b) = self.param_count_b {
            model.param_count_b = param_count_b;
        }
        if let Some(architecture) = &self.architecture {
            model.architecture = Some(architecture.clone());
        }
        if let Some(context_length) = self.context_length {
            model.context_length = Some(context_length);
        }
        if let Some(metadata) = &self.metadata {
            model.metadata.clone_from(metadata);
        }
        if let Some(defaults) = &self.inference_defaults {
            // An empty config sets nothing, so it is stored as no config and
            // the model inherits: there is no value left to have an origin.
            // Anything else is a person's choice from here on, whatever
            // numbers it lands on. See `DefaultsOrigin`.
            let kept = (*defaults != InferenceConfig::default()).then(|| defaults.clone());
            model.defaults_origin = kept.as_ref().map(|_| DefaultsOrigin::User);
            model.inference_defaults = kept;
        }
        // `None` leaves the stored defaults alone; `Some(None)` clears them.
        if let Some(server_defaults) = &self.server_defaults {
            model.server_defaults.clone_from(server_defaults);
        }
    }
}

#[cfg(test)]
#[path = "types_model_update_tests.rs"]
mod tests;
