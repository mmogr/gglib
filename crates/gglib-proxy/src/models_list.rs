//! The `/v1/models` list: every catalogued model, in `OpenAI`'s shape.
//!
//! A `#[path]` child of `models.rs`, which re-exports both types. Built here
//! from the catalogue's summaries; `models_endpoint.rs` serves it, and
//! `profiles.rs` adds a `{model}:{profile}` entry for each profile listed in
//! models.

use gglib_core::ports::ModelSummary;
use gglib_core::server_config::{
    ContextSizeSource, ServerConfigOptions, resolve_context_size_with_source,
};
use serde::Serialize;

/// Response from /v1/models endpoint.
#[derive(Debug, Clone, Serialize)]
pub struct ModelsResponse {
    pub object: String,
    pub data: Vec<ModelInfo>,
}

impl ModelsResponse {
    /// Create a new `ModelsResponse` from a list of model summaries.
    ///
    /// Each model's `context_window` is the GGUF's trained ceiling, capped by
    /// anything a person actually configured — a per-model server default or a
    /// global setting, resolved through
    /// [`resolve_context_size_with_source`].
    ///
    /// A chain that falls through to the built-in floor means nothing is
    /// configured. Where the launch will fit the context to this machine, no
    /// cap is applied: advertising 4096 would understate what is about to be
    /// served, and the trained window is a true upper bound because
    /// `fit_context` caps at it before snapping. Where it will not —
    /// `fit_available` is false, so the floor is what a launch actually gets —
    /// the floor is what gets advertised.
    pub fn from_summaries(
        summaries: Vec<ModelSummary>,
        global_default_ctx: Option<u64>,
        fit_available: bool,
    ) -> Self {
        let data: Vec<ModelInfo> = summaries
            .into_iter()
            .map(|summary| {
                let (effective_cap, cap_source) =
                    resolve_context_size_with_source(&ServerConfigOptions {
                        context_size: None,
                        model_server_ctx: summary
                            .server_defaults
                            .as_ref()
                            .and_then(|sd| sd.context_length),
                        global_default_ctx,
                        ..Default::default()
                    });
                ModelInfo {
                    id: summary.name.clone(),
                    object: "model".to_string(),
                    created: summary.created_at,
                    owned_by: "gglib".to_string(),
                    description: Some(summary.description()),
                    // Capped by what a person configured — or, where no fit
                    // is reachable, by the floor that will therefore be
                    // served. Advertising the trained window on a host gglib
                    // cannot probe overstates it by up to 32x, read once.
                    context_window: match cap_source {
                        ContextSizeSource::BuiltInDefault if fit_available => {
                            summary.context_length
                        }
                        _ => summary.context_length.map(|ctx| ctx.min(effective_cap)),
                    },
                    capabilities: capabilities_of(&summary),
                }
            })
            .collect();
        Self {
            object: "list".to_string(),
            data,
        }
    }
}

/// The extra endpoints a catalogued model can serve, for
/// [`ModelInfo::capabilities`].
///
/// `None` rather than an empty vec for an ordinary chat model, so the field
/// disappears from the response instead of appearing as `[]` — an empty list
/// reads as "this model can do nothing", which is the opposite of the truth.
fn capabilities_of(summary: &ModelSummary) -> Option<Vec<String>> {
    summary
        .tags
        .iter()
        .any(|t| t == crate::embeddings::EMBEDDING_TAG)
        .then(|| vec!["embeddings".to_string()])
}

/// Information about a single model (`OpenAI` format).
#[derive(Debug, Clone, Serialize)]
pub struct ModelInfo {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub owned_by: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Model's context window size, in tokens (llama.cpp's `/v1/models`
    /// field-naming convention). `None` when unknown.
    ///
    /// Set by [`ModelsResponse::from_summaries`], then adjusted by
    /// [`crate::models_endpoint::list_models`]: the running model is
    /// overwritten with its live `effective_ctx`, and every entry is shaved by
    /// the advertisement's safety margin. Clients that auto-detect context
    /// size read this once at picker-build time — usually before any model
    /// runs — so the pre-launch value must already be honest.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u64>,
    /// Non-OpenAI endpoints this model can serve, beyond
    /// `/v1/chat/completions`.
    ///
    /// `Some(["embeddings"])` for a model tagged `embedding`; `None` — and so
    /// absent from the JSON entirely — for everything else. A chat client's
    /// picker is therefore byte-identical to what it saw before this field
    /// existed, while a RAG client has something to filter on other than
    /// guessing from the model's name.
    ///
    /// An array rather than a `type` discriminant because capability is not
    /// exclusive: a future entry may serve both chat and embeddings, and
    /// vision or tool support could join the same list without a second field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<Vec<String>>,
}

#[cfg(test)]
#[path = "models_list_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "models_list_conversion_tests.rs"]
mod conversion_tests;
