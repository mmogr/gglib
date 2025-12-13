//! Application-level events (model lifecycle).

use serde::{Deserialize, Serialize};

use super::AppEvent;

/// Summary of a model for event payloads.
///
/// This is a lightweight representation for events — not the full `Model`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelSummary {
    /// Database ID of the model.
    pub id: i64,
    /// Human-readable model name.
    pub name: String,
    /// File path to the model.
    pub file_path: String,
    /// Model architecture (e.g., "llama").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub architecture: Option<String>,
    /// Quantization type (e.g., "`Q4_0`").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantization: Option<String>,
}

impl ModelSummary {
    /// Create a new model summary.
    pub fn new(
        id: i64,
        name: impl Into<String>,
        file_path: impl Into<String>,
        architecture: Option<String>,
        quantization: Option<String>,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            file_path: file_path.into(),
            architecture,
            quantization,
        }
    }
}

impl AppEvent {
    /// Create a model added event.
    pub const fn model_added(model: ModelSummary) -> Self {
        Self::ModelAdded { model }
    }

    /// Create a model removed event.
    pub const fn model_removed(model_id: i64) -> Self {
        Self::ModelRemoved { model_id }
    }

    /// Create a model updated event.
    pub const fn model_updated(model: ModelSummary) -> Self {
        Self::ModelUpdated { model }
    }
}
