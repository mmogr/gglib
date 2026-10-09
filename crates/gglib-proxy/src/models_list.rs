//! The `/v1/models` list: every catalogued model, in `OpenAI`'s shape.
//!
//! A `#[path]` child of `models.rs`, which re-exports both types. Built here
//! from the catalogue's summaries; `models_endpoint.rs` serves it, and
//! `profiles.rs` adds a `{model}:{profile}` entry for each profile listed in
//! models.
//!
//! Both types also deserialize, for a reader of this list on another machine,
//! into the same shape it was written from.

use gglib_core::domain::capability_tags;
use gglib_core::ports::ModelSummary;
use gglib_core::server_config::{
    ContextSizeSource, ServerConfigOptions, resolve_context_size_with_source,
};
use serde::{Deserialize, Serialize};

/// Response from /v1/models endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct ModelsResponse {
    pub object: String,
    pub data: Vec<ModelInfo>,
    /// This machine's name: the first label of its host name, read on every
    /// request and kept only when it is a plain host label
    /// ([`gglib_core::domain::machine_name`]). `None` when the host name is
    /// unreadable or is not one. Not part of `OpenAI`'s shape; a client that
    /// does not know it ignores it.
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub machine_name: Option<String>,
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
                    gglib_id: i64::from(summary.id),
                    profile: None,
                    object: "model".to_string(),
                    created: summary.created_at,
                    owned_by: "gglib".to_string(),
                    description: Some(summary.description()),
                    // Capped by what a person configured — or, where no fit
                    // is reachable, by the floor that will therefore be
                    // served. Advertising the trained window on a host gglib
                    // cannot probe overstates it by up to 32x, read once.
                    // A model that draws has no chat context at all, whatever
                    // its row says.
                    context_window: match cap_source {
                        _ if summary.image_output => None,
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
            machine_name: None,
        }
    }
}

/// This machine's name, the first label of its host name.
///
/// `None` when [`gglib_core::domain::machine_name`] does not keep it. What
/// [`ModelsResponse::machine_name`] publishes, and what a pairing tells the
/// machine it pairs with this one is called.
#[must_use]
pub fn this_machine_name() -> Option<String> {
    sysinfo::System::host_name()
        .as_deref()
        .and_then(gglib_core::domain::machine_name)
}

/// What [`ModelInfo::capabilities`] lists for a model that reads images: one
/// linked to a projector. The `OpenAI`-side spelling of
/// [`ModelSummary::image_input`], and what a client reads to offer images.
pub const VISION_CAPABILITY: &str = "vision";

/// What [`ModelInfo::capabilities`] lists for a model that draws images.
///
/// One whose weights name an image family: the `OpenAI`-side spelling of
/// [`ModelSummary::image_output`]. Such a model is refused for chat.
pub const IMAGE_GENERATION_CAPABILITY: &str = "image_generation";

/// What [`ModelInfo::capabilities`] lists for a model that thinks: one
/// tagged `reasoning`, which is what launches it with its thinking set apart
/// from its answer. The tag, not the template-derived capability bit, and
/// what a client reads to offer a Thinking switch.
pub(crate) const REASONING_CAPABILITY: &str = "reasoning";

/// What a catalogued model can do beyond text chat, for
/// [`ModelInfo::capabilities`]: serve embeddings, read images, draw images,
/// think.
///
/// `None` rather than an empty vec for an ordinary chat model, so the field
/// disappears from the response instead of appearing as `[]` — an empty list
/// reads as "this model can do nothing", which is the opposite of the truth.
fn capabilities_of(summary: &ModelSummary) -> Option<Vec<String>> {
    let embeddings = capability_tags::is_embedding(&summary.tags).then_some("embeddings");
    let vision = summary.image_input.then_some(VISION_CAPABILITY);
    let draws = summary.image_output.then_some(IMAGE_GENERATION_CAPABILITY);
    let reasoning = capability_tags::is_reasoning(&summary.tags).then_some(REASONING_CAPABILITY);
    let capabilities: Vec<String> = embeddings
        .into_iter()
        .chain(vision)
        .chain(draws)
        .chain(reasoning)
        .map(str::to_owned)
        .collect();
    (!capabilities.is_empty()).then_some(capabilities)
}

/// Information about a single model (`OpenAI` format).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct ModelInfo {
    /// What a client sends back as `model`: the model's name, or
    /// `{name}:{profile}` for a profile variant.
    pub id: String,
    /// The model's id in this machine's catalogue, which is never reused.
    /// A profile variant carries its base model's.
    ///
    /// Not the `id` above, which is what a client sends: a catalogue id means
    /// something only on the machine that issued it.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub gglib_id: i64,
    /// The inference profile a `{name}:{profile}` variant selects; `None` on
    /// a base entry.
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    pub object: String,
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub created: i64,
    pub owned_by: String,
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Model's context window size, in tokens (llama.cpp's `/v1/models`
    /// field-naming convention). `None` when unknown, and for a model that
    /// draws images, which has no chat context.
    ///
    /// Set by [`ModelsResponse::from_summaries`], then adjusted by
    /// [`crate::models_endpoint::list_models`]: the running model is
    /// overwritten with its live `effective_ctx`, and every entry is shaved by
    /// the advertisement's safety margin. Clients that auto-detect context
    /// size read this once at picker-build time — usually before any model
    /// runs — so the pre-launch value must already be honest.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number", optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u64>,
    /// What this model can do beyond text chat.
    ///
    /// `"embeddings"` for a model tagged `embedding`, which serves
    /// `/v1/embeddings`; `"vision"` for a model linked to a projector, which
    /// reads `image_url` parts; `"image_generation"` for a model that draws
    /// images, which is refused for chat; `"reasoning"` for a model tagged
    /// `reasoning`, for which a client may offer a Thinking switch. In that
    /// order. `None`
    /// — and so absent from the JSON entirely — for a model that is none of
    /// them, so a plain chat model's entry is byte-identical to what it was
    /// before this field existed.
    ///
    /// An array rather than a `type` discriminant because capability is not
    /// exclusive: a future entry may serve both chat and embeddings, and
    /// tool support could join the same list without a second field.
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<Vec<String>>,
}

#[cfg(test)]
#[path = "models_list_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "models_list_conversion_tests.rs"]
mod conversion_tests;
