//! Resolved sampling parameters plus the layer that supplied each one.
//!
//! The wire form of [`FieldSources`](gglib_core::domain::FieldSources), which the surfaces cannot serialize
//! directly: [`ParamSource::Layer`] carries a ladder index rather than a name,
//! and the core provenance types predate the camelCase convention the rest of
//! the HTTP API follows. Translating once here keeps a single description of
//! the shape instead of one per surface.
//!
//! Nothing in this module re-derives the ladder. The values and their
//! provenance both come from
//! [`InferenceConfig::resolve_with_profile_explained`] — the same call the
//! plain resolution makes — so an explanation cannot describe a hierarchy that
//! differs from the one that runs.

use gglib_core::domain::{
    DefaultsOrigin, InferenceConfig, InferenceProfile, Model, ModelSamplingContext,
    ModelSamplingDefaults, ParamSource, ReasoningEffort, SamplingLayer, SamplingOverride,
};
use gglib_core::settings::Settings;
use serde::{Deserialize, Serialize};

use crate::error::GuiError;

/// A model's resolved sampling parameters and where each value came from.
///
/// Consumers:
/// - Axum: `GET /api/models/:id/explain`
/// - GUI frontend: the inspector's Sampling section
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct SamplingExplanationDto {
    /// The fully resolved configuration, after floors.
    ///
    /// Carried whole rather than copied field-by-field into
    /// [`ParamProvenanceDto`], so each value keeps the width it was declared
    /// at and `serde_json` emits the shortest representation that round-trips
    /// — `0.95`, not the `0.949999988079071` a detour through `f64` produces.
    pub resolved: InferenceConfig,
    /// Per-parameter provenance, in [`FieldSources::iter`](gglib_core::domain::FieldSources::iter)'s display order.
    pub sources: Vec<ParamProvenanceDto>,
    /// The profile applied, if the caller named one.
    pub profile: Option<String>,
    /// Whether the model carries the `reasoning` tag, which selects the floor.
    pub is_reasoning: bool,
    /// Whether client-supplied sampling is trusted, which the table cannot
    /// show: that rung is never stored on the model.
    pub trust_client_sampling: bool,
    /// What this model's own GGUF publishes, for the fields where it publishes
    /// anything at all.
    ///
    /// Empty on almost every model, and empty is the *interesting* default:
    /// carrying a `notPublished` entry per field would put five noise rows on
    /// every ordinary model to say nothing. A parameter absent from this list
    /// has no author recommendation to honour or displace.
    ///
    /// The provenance column alone cannot answer this. Since llama.cpp PR
    /// #17120 a `general.sampling.*` key is the server's default for every
    /// field gglib does not name, so `unset` means *the model's own number
    /// applies* on a model that published one, and *the build's default
    /// applies* on a model that did not — and those render identically without
    /// this.
    #[serde(default)]
    pub published: Vec<PublishedDefaultDto>,
    /// Where the model's stored defaults came from.
    ///
    /// `Published` and `AutoDetected` share a ladder rung — both are
    /// unreviewed, so both rank below global settings — which means
    /// [`ParamProvenanceDto::layer`] alone cannot name its own source. Without
    /// this, a recipe fetched from the model author renders as gglib's
    /// reasoning-tag guess.
    ///
    /// The domain enum itself, not a re-spelling of it: the library list
    /// reports this same column, and a wire form that differed from it in
    /// casing alone meant one stored value reached a client under two names.
    #[serde(default)]
    pub defaults_origin: Option<DefaultsOrigin>,
    /// The `reasoning_effort` this model's template would ignore, when the
    /// stored configuration resolves one it does not read.
    ///
    /// # Why a table row is not enough
    ///
    /// The suppression *is* in [`Self::sources`] — that entry reads
    /// [`SuppressedByTemplate`](ProvenanceKindDto::SuppressedByTemplate) — and
    /// [`Self::resolved`] carries `null`, because that is what would be sent.
    /// Between them a client can say *something was suppressed* and nothing
    /// more: the level is gone from `resolved`, and the rung that asked for it
    /// has been overwritten by the marker. Those two are the whole content of
    /// the sentence a reader needs — "the `:high` profile asks for `high`; this
    /// model's template does not read `reasoning_effort`" — so they are carried
    /// here or they are carried nowhere.
    ///
    /// # It describes a request that has not happened
    ///
    /// This endpoint explains *stored configuration*. Nothing has been sent, so
    /// this is a conditional: on any request against this model, that is what
    /// would happen. A surface must word it that way. The unconditional twin
    /// lives on the proxy's sampling audit, where a request really did run.
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort_suppressed: Option<SuppressedEffortDto>,
}

/// A resolved `reasoning_effort` this model's template would not read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct SuppressedEffortDto {
    /// The level the ladder resolved.
    pub level: ReasoningEffort,
    /// The rung that supplied it, before the suppression overwrote the entry
    /// in [`SamplingExplanationDto::sources`].
    ///
    /// `None` only for a ladder index this module cannot name, which the five
    /// rungs it resolves cannot produce — and never for a floor, because no
    /// floor names an effort.
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layer: Option<SamplingLayerDto>,
}

/// What gglib does with one field's published recommendation.
///
/// The wire form of [`SamplingOverride`],
/// minus its `NotPublished` arm — a field with nothing published is omitted
/// from [`SamplingExplanationDto::published`] rather than carried as an empty
/// verdict.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct PublishedDefaultDto {
    /// The camelCase key this entry describes, e.g. `topP`. Joins to
    /// [`ParamProvenanceDto::param`] so a surface can render the two together.
    pub param: String,
    /// The GGUF key carrying the value, e.g. `general.sampling.temp`.
    ///
    /// Shown rather than derived, because `repeat_penalty` is published under
    /// `general.sampling.penalty_repeat` and no client should have to know
    /// that.
    pub key: String,
    /// What gglib is doing about it.
    #[serde(flatten)]
    pub state: PublishedStateDto,
}

/// The verdict arm of [`PublishedDefaultDto`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum PublishedStateDto {
    /// gglib names nothing, so llama.cpp applies the model author's value.
    Deferred {
        /// What the model author published, and what the sampler will use.
        published: f64,
    },
    /// gglib sends the same number the model published.
    Restated {
        /// The value both sides name.
        published: f64,
    },
    /// gglib sends a different number than the model published.
    ///
    /// The one arm a surface should render as a warning.
    Overridden {
        /// What the model author published.
        published: f64,
        /// What gglib puts on the wire instead.
        sending: f64,
    },
    /// The model names the key and gglib could not read its value.
    ///
    /// Never rendered as an override: gglib cannot tell what it displaced, and
    /// claiming otherwise is the mistake [ADR 0004] decision 3 forbids one
    /// layer up.
    ///
    /// [ADR 0004]: https://github.com/mmogr/gglib/blob/main/docs/adr/0004-observe-the-sampling-boundary.md
    Unreadable,
}

/// Where one resolved parameter's value came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ParamProvenanceDto {
    /// The camelCase key this entry describes in
    /// [`SamplingExplanationDto::resolved`], e.g. `topP`.
    pub param: String,
    /// Which class of source supplied the value.
    pub kind: ProvenanceKindDto,
    /// The ladder rung, present only when `kind` is
    /// [`Layer`](ProvenanceKindDto::Layer).
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layer: Option<SamplingLayerDto>,
}

/// The wire form of [`ParamSource`], with the ladder index resolved to a name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum ProvenanceKindDto {
    /// A ladder rung named the value; see [`ParamProvenanceDto::layer`].
    Layer,
    /// The class floor, because no rung named a value at all.
    Floor,
    /// The class floor, because a rung claimed `temperature` and this
    /// parameter is tuned against it.
    FloorCoupled,
    /// Nothing named it and the floor carries none either.
    Unset,
    /// A rung named the value and a request-shaping stage suppressed it,
    /// because the model's observed template does not read the field
    /// (ADR 0007). Unreachable from this module, which explains stored
    /// configuration with no request in hand — but the DTO mirrors
    /// [`ParamSource`] and a mirror missing an arm is how a surface starts
    /// rendering a suppressed value as if it had been sent.
    SuppressedByTemplate,
}

/// The wire form of [`SamplingLayer`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum SamplingLayerDto {
    /// Caller-supplied overrides — request parameters or CLI flags.
    Request,
    /// The named profile the caller selected.
    Profile,
    /// Per-model defaults a person tuned deliberately.
    ModelUserSet,
    /// Global settings defaults.
    Global,
    /// Per-model defaults written automatically from the `reasoning` tag.
    ModelAutoDetected,
}

impl SamplingLayerDto {
    /// The `const` half of the [`From`] impl below, so [`layer_of`] can stay
    /// `const` — `From::from` is not callable in a `const fn`.
    const fn from_layer(layer: SamplingLayer) -> Self {
        match layer {
            SamplingLayer::Request => Self::Request,
            SamplingLayer::Profile => Self::Profile,
            SamplingLayer::ModelUserSet => Self::ModelUserSet,
            SamplingLayer::Global => Self::Global,
            SamplingLayer::ModelAutoDetected => Self::ModelAutoDetected,
        }
    }
}

impl From<SamplingLayer> for SamplingLayerDto {
    fn from(layer: SamplingLayer) -> Self {
        Self::from_layer(layer)
    }
}

/// The camelCase key one [`FieldSources`](gglib_core::domain::FieldSources) field occupies in a serialized
/// [`InferenceConfig`].
///
/// A field this does not know is returned unchanged, which produces a `param`
/// that indexes nothing rather than a panic in a read-only view. The pairing
/// is pinned by `every_param_is_a_key_of_the_resolved_config` below, so a new
/// field on `FieldSources` fails a test rather than reaching a client.
fn wire_key(field: &'static str) -> &'static str {
    match field {
        "top_p" => "topP",
        "top_k" => "topK",
        "presence_penalty" => "presencePenalty",
        "repeat_penalty" => "repeatPenalty",
        "min_p" => "minP",
        "frequency_penalty" => "frequencyPenalty",
        "dynatemp_range" => "dynatempRange",
        "dynatemp_exponent" => "dynatempExponent",
        "top_n_sigma" => "topNSigma",
        "dry_multiplier" => "dryMultiplier",
        "dry_base" => "dryBase",
        "dry_allowed_length" => "dryAllowedLength",
        "dry_penalty_last_n" => "dryPenaltyLastN",
        "max_tokens" => "maxTokens",
        "reasoning_effort" => "reasoningEffort",
        "reasoning_budget_tokens" => "reasoningBudgetTokens",
        other => other,
    }
}

/// The named rung a source came from, when it came from a rung at all.
///
/// The one place this module turns a ladder index into a name, so
/// [`provenance`] and the suppression record cannot disagree about which rung
/// `Layer(1)` is. Exhaustive rather than catch-all: a new [`ParamSource`] arm
/// must be decided here rather than silently reported as nameless.
///
/// `from_index` returns `None` only for a ladder longer than the five rungs
/// `resolve_with_profile_explained` builds, which this module never resolves.
/// Surfaced as a nameless rung rather than guessed at.
const fn layer_of(source: ParamSource) -> Option<SamplingLayerDto> {
    match source {
        ParamSource::Layer(index) => match SamplingLayer::from_index(index) {
            Some(layer) => Some(SamplingLayerDto::from_layer(layer)),
            None => None,
        },
        // No floor names a value it can attribute, an unset field names none at
        // all, and a suppression has deliberately overwritten the rung it
        // replaced — see `SamplingExplanationDto::effort_suppressed` for where
        // that rung goes instead.
        ParamSource::Floor
        | ParamSource::FloorCoupled
        | ParamSource::Unset
        | ParamSource::SuppressedByTemplate => None,
    }
}

/// Translate one `(field, source)` pair into its wire form.
fn provenance(field: &'static str, source: ParamSource) -> ParamProvenanceDto {
    let kind = match source {
        ParamSource::Layer(_) => ProvenanceKindDto::Layer,
        ParamSource::Floor => ProvenanceKindDto::Floor,
        ParamSource::FloorCoupled => ProvenanceKindDto::FloorCoupled,
        ParamSource::Unset => ProvenanceKindDto::Unset,
        ParamSource::SuppressedByTemplate => ProvenanceKindDto::SuppressedByTemplate,
    };

    ParamProvenanceDto {
        param: wire_key(field).to_owned(),
        kind,
        layer: layer_of(source),
    }
}

/// Look up a configured profile by name.
///
/// Errors rather than falling back to no profile: someone who named a profile
/// wants to see that profile's effect, and silently showing them the
/// unprofiled resolution would answer a question they did not ask. The message
/// names what does exist, so a typo is self-correcting.
pub(crate) fn find_profile<'a>(
    name: &str,
    profiles: Option<&'a [InferenceProfile]>,
) -> Result<&'a InferenceProfile, GuiError> {
    let profiles = profiles.unwrap_or_default();
    profiles
        .iter()
        .find(|profile| profile.name == name)
        .ok_or_else(|| {
            let names = profiles
                .iter()
                .map(|profile| profile.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            GuiError::ValidationFailed(if names.is_empty() {
                format!("no profile named '{name}'; none are configured")
            } else {
                format!("no profile named '{name}'; configured profiles are: {names}")
            })
        })
}

/// Resolve a model's sampling parameters and describe where each came from.
pub(crate) fn explain(
    model: &Model,
    settings: &Settings,
    profile: Option<&InferenceProfile>,
) -> SamplingExplanationDto {
    // The two facts about the model that change how resolution behaves.
    let model_ctx = ModelSamplingContext::for_model(model);

    // An empty request layer: this explains the stored configuration, so there
    // are no per-request parameters to occupy the top rung.
    let (mut resolved, mut sources) = InferenceConfig::default().resolve_with_profile_explained(
        profile.map(|selected| &selected.config),
        model.inference_defaults.as_ref(),
        settings.inference_defaults.as_ref(),
        model_ctx,
    );

    // Stage 5b's rule, applied to the resolution rather than to a request. The
    // shared predicate, not a copy of it: an explanation that re-derived the
    // condition could only ever disagree with the gate it is describing, and
    // would then be a confident account of something that does not happen. A
    // no-op on every model whose caps were never read — which is most of them,
    // and is `Unknown`, not `No`.
    let effort_suppressed = gglib_core::request_pipeline::suppress_stored_effort(
        &mut resolved,
        &mut sources,
        &model.template_caps,
    )
    .map(|suppressed| SuppressedEffortDto {
        level: suppressed.level,
        layer: layer_of(suppressed.source),
    });

    // What gglib actually puts on the wire, read from the patch the request
    // pipeline merges into the body rather than from the struct fields. A
    // parameter missing from this map is one gglib names nowhere, which is
    // exactly the condition under which the model's own GGUF value survives to
    // the sampler.
    let patch = resolved.to_openai_json_patch();
    let published = ModelSamplingDefaults::from_metadata(&model.metadata)
        .compare_all(|field| patch.get(field).and_then(serde_json::Value::as_f64))
        .into_iter()
        .filter_map(|(field, verdict)| published_default(field, &verdict))
        .collect();

    SamplingExplanationDto {
        resolved,
        sources: sources
            .iter()
            .map(|(field, source)| provenance(field, source))
            .collect(),
        profile: profile.map(|selected| selected.name.clone()),
        is_reasoning: model_ctx.is_reasoning,
        trust_client_sampling: settings.trust_client_sampling.unwrap_or(false),
        published,
        defaults_origin: model.defaults_origin,
        effort_suppressed,
    }
}

/// Translate one override verdict into its wire form.
///
/// `None` for [`SamplingOverride::NotPublished`], which is how a field with no
/// author recommendation stays out of the payload entirely.
fn published_default(
    field: &'static str,
    verdict: &SamplingOverride,
) -> Option<PublishedDefaultDto> {
    let (key, state) = match verdict {
        SamplingOverride::NotPublished => return None,
        SamplingOverride::Deferred { key, published } => (
            *key,
            PublishedStateDto::Deferred {
                published: *published,
            },
        ),
        SamplingOverride::Restated { key, published } => (
            *key,
            PublishedStateDto::Restated {
                published: *published,
            },
        ),
        SamplingOverride::Overridden {
            key,
            published,
            sending,
        } => (
            *key,
            PublishedStateDto::Overridden {
                published: *published,
                sending: *sending,
            },
        ),
        SamplingOverride::Unreadable { key, .. } => (*key, PublishedStateDto::Unreadable),
    };
    Some(PublishedDefaultDto {
        param: wire_key(field).to_owned(),
        key: key.to_owned(),
        state,
    })
}

#[cfg(test)]
#[path = "sampling_explain_tests.rs"]
mod sampling_explain_tests;
