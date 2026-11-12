//! Which layer supplied each resolved sampling parameter.
//!
//! ## Why this exists
//!
//! [`InferenceConfig::resolve_layers_with_sources`](crate::domain::InferenceConfig::resolve_layers_with_sources)
//! folds an ordered ladder of sampling layers into one config under two rules
//! that are individually defensible and jointly opaque: the coupled trio
//! (`presence_penalty`, `repeat_penalty`, `min_p`) travels with the
//! `temperature` it was tuned against, and a model's stored defaults rank
//! above or below global settings depending on whether a person set them.
//!
//! The resolved numbers alone cannot distinguish a value someone chose from
//! one that was passed over. "No `presence_penalty` sent" is an absence;
//! "none sent, because the profile claimed the temperature and the model's
//! 1.5 was tuned for another" is an explanation, and only the second makes
//! the behaviour auditable.
//!
//! ## One computation, not two
//!
//! [`FieldSources`] is produced by
//! [`resolve_layers_with_sources`](crate::domain::InferenceConfig::resolve_layers_with_sources),
//! the same pass that decides the values — never by a second function that
//! re-derives the rules. That is deliberate: two implementations of one rule
//! drift, and provenance that names a rung the resolution did not use is
//! wrong without anything failing.
//!
//! The same `(value, source)` shape
//! [`resolve_context_size_with_source`](crate::server_config::resolve_context_size_with_source)
//! uses, and for the same reason.

use serde::{Deserialize, Serialize};

use super::inference::DefaultsOrigin;

/// Which rung of a sampling ladder supplied one resolved parameter.
///
/// [`Layer`](Self::Layer) carries an index into the ladder that was resolved,
/// rather than a name, because the ladders differ. [`SamplingLayer`] describes
/// the five-rung ladder
/// [`resolve_with_profile_explained`](crate::domain::InferenceConfig::resolve_with_profile_explained)
/// builds; the request pipeline builds a **six**-rung one, adding `cli` and
/// `client` above the rest. Callers map the index back to whatever names their
/// own ladder used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParamSource {
    /// The layer at this index in the resolved ladder named the value.
    Layer(usize),
    /// A layer claimed `temperature`, this parameter is tuned against it, and
    /// neither that layer nor any above it named one, so no value is sent.
    ///
    /// Distinct from [`Unset`](Self::Unset) in the fact that matters: here a
    /// lower layer may well have named a value and was **deliberately passed
    /// over**. That is what the coupling rule does, and it is the one thing an
    /// absent key can never explain. "A value was discarded here" and "nobody
    /// ever named one" are different explanations, and only the first tells a
    /// reader where to look.
    Coupled,
    /// Nothing named it, so no value is sent and llama.cpp's own default
    /// applies. There is no floor beneath the ladder (ADR 0022).
    Unset,
}

/// The five rungs of the ladder
/// [`resolve_with_profile`](crate::domain::InferenceConfig::resolve_with_profile)
/// builds, in priority order.
///
/// [`ModelUserSet`](Self::ModelUserSet) and
/// [`ModelAutoDetected`](Self::ModelAutoDetected) are both the model's own:
/// what a person set, and the recipe beneath global settings.
/// [`ModelSamplingContext::model_rungs`](crate::domain::ModelSamplingContext::model_rungs)
/// decides what each holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SamplingLayer {
    /// Caller-supplied overrides — request parameters or CLI flags.
    Request,
    /// The named profile the caller selected.
    Profile,
    /// Per-model defaults a person tuned deliberately.
    ModelUserSet,
    /// Global settings defaults.
    Global,
    /// Per-model defaults no person in this installation chose, so they rank
    /// below global settings. Three recipes share this rung by replacement —
    /// the `reasoning`-tag guess, the model author's published recipe, and a
    /// tune sweep's measured winner — and `DefaultsOrigin` says which stored
    /// one is in it, or none for the `reasoning` recipe; the surfaces that
    /// name the rung read that, not this.
    ModelAutoDetected,
}

impl SamplingLayer {
    /// The rung at `index` in the ladder `resolve_with_profile` builds.
    ///
    /// The mapping lives here rather than at each call site so a change to the
    /// ladder's order cannot silently mislabel a `ParamSource::Layer`.
    #[must_use]
    pub const fn from_index(index: usize) -> Option<Self> {
        match index {
            0 => Some(Self::Request),
            1 => Some(Self::Profile),
            2 => Some(Self::ModelUserSet),
            3 => Some(Self::Global),
            4 => Some(Self::ModelAutoDetected),
            _ => None,
        }
    }
}

/// Where one parameter of the five-rung ladder ([`SamplingLayer`]) came
/// from, in a person's words: what `gglib model explain` prints beside the
/// value, and the inspector and the sampling form show.
///
/// `profile` is the profile applied, which the profile rung is named by.
/// `origin` is where the model's stored defaults came from: three recipes
/// share the auto-detected rung (the `reasoning`-tag guess, the model
/// author's published one and a tune sweep's), and only it tells them
/// apart. A user-set rung reads as one whatever the origin.
#[must_use]
pub fn source_label(
    source: ParamSource,
    profile: Option<&str>,
    origin: Option<DefaultsOrigin>,
) -> String {
    let layer = match source {
        ParamSource::Layer(index) => SamplingLayer::from_index(index),
        ParamSource::Coupled => return "unset (coupled to temperature layer)".to_owned(),
        ParamSource::Unset => return "unset by design".to_owned(),
    };
    match layer {
        Some(SamplingLayer::Request) => "request parameters".to_owned(),
        Some(SamplingLayer::Profile) => {
            profile.map_or_else(|| "profile".to_owned(), |name| format!("profile '{name}'"))
        }
        Some(SamplingLayer::ModelUserSet) => "per-model defaults (user-set)".to_owned(),
        Some(SamplingLayer::Global) => "global settings".to_owned(),
        Some(SamplingLayer::ModelAutoDetected) => match origin {
            Some(DefaultsOrigin::Published) => {
                "per-model defaults (published by the model author)".to_owned()
            }
            Some(DefaultsOrigin::Measured) => {
                "per-model defaults (measured by a tune sweep)".to_owned()
            }
            _ => "per-model defaults (auto-detected: reasoning tag)".to_owned(),
        },
        // A ladder longer than the five rungs, which none resolved here is.
        None => "an unnamed layer".to_owned(),
    }
}

/// Per-field provenance for one resolved [`InferenceConfig`].
///
/// [`InferenceConfig`]: crate::domain::InferenceConfig
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldSources {
    /// Where the resolved `temperature` came from.
    pub temperature: ParamSource,
    /// Where the resolved `top_p` came from.
    pub top_p: ParamSource,
    /// Where the resolved `top_k` came from.
    pub top_k: ParamSource,
    /// Where the resolved `presence_penalty` came from.
    pub presence_penalty: ParamSource,
    /// Where the resolved `repeat_penalty` came from.
    pub repeat_penalty: ParamSource,
    /// Where the resolved `min_p` came from.
    pub min_p: ParamSource,
    /// Where the resolved `frequency_penalty` came from.
    pub frequency_penalty: ParamSource,
    /// Where the resolved `dynatemp_range` came from.
    pub dynatemp_range: ParamSource,
    /// Where the resolved `dynatemp_exponent` came from.
    pub dynatemp_exponent: ParamSource,
    /// Where the resolved `top_n_sigma` came from.
    pub top_n_sigma: ParamSource,
    /// Where the resolved `dry_multiplier` came from.
    pub dry_multiplier: ParamSource,
    /// Where the resolved `dry_base` came from.
    pub dry_base: ParamSource,
    /// Where the resolved `dry_allowed_length` came from.
    pub dry_allowed_length: ParamSource,
    /// Where the resolved `dry_penalty_last_n` came from.
    pub dry_penalty_last_n: ParamSource,
    /// Where the resolved `max_tokens` came from.
    pub max_tokens: ParamSource,
    /// Where the resolved `reasoning_effort` came from.
    ///
    /// The only account there is. Neither reasoning control is observable at
    /// the sampling boundary — no `/slots` or `/props` field echoes either one
    /// ([ADR 0007] finding 7a) — so where a readback can eventually confirm
    /// that `top_k` arrived, nothing will ever confirm this did. Provenance is
    /// not a convenience for these two fields; it is the record.
    ///
    /// [ADR 0007]: https://github.com/mmogr/gglib/blob/main/docs/adr/0007-ask-the-server-for-template-capabilities.md
    pub reasoning_effort: ParamSource,
    /// Where the resolved `reasoning_budget_tokens` came from. Equally
    /// unobservable — see [`reasoning_effort`](Self::reasoning_effort).
    pub reasoning_budget_tokens: ParamSource,
}

impl FieldSources {
    /// `(field_name, source)` pairs in display order.
    ///
    /// The single iteration order every consumer renders, so the CLI's table
    /// and the pipeline's debug line cannot disagree about which parameter is
    /// which. The coupled trio is kept adjacent because it is only
    /// interpretable as a group.
    pub fn iter(&self) -> impl Iterator<Item = (&'static str, ParamSource)> {
        [
            ("temperature", self.temperature),
            ("top_p", self.top_p),
            ("top_k", self.top_k),
            ("presence_penalty", self.presence_penalty),
            ("repeat_penalty", self.repeat_penalty),
            ("min_p", self.min_p),
            ("frequency_penalty", self.frequency_penalty),
            ("dynatemp_range", self.dynatemp_range),
            ("dynatemp_exponent", self.dynatemp_exponent),
            ("top_n_sigma", self.top_n_sigma),
            ("dry_multiplier", self.dry_multiplier),
            ("dry_base", self.dry_base),
            ("dry_allowed_length", self.dry_allowed_length),
            ("dry_penalty_last_n", self.dry_penalty_last_n),
            ("max_tokens", self.max_tokens),
            // Last, and adjacent to each other rather than to the samplers:
            // neither is one. `max_tokens` above them is the nearest relative
            // — a budget — and `reasoning_budget_tokens` sits directly beneath
            // it for that reason.
            ("reasoning_effort", self.reasoning_effort),
            ("reasoning_budget_tokens", self.reasoning_budget_tokens),
        ]
        .into_iter()
    }

    /// Render as `field=layer` pairs against the ladder's own layer names.
    ///
    /// `names` is indexed by [`ParamSource::Layer`]; an index past its end
    /// renders as `?`, which can only happen if a caller passes names for a
    /// different ladder than it resolved.
    #[must_use]
    pub fn describe(&self, names: &[&str]) -> String {
        self.iter()
            .map(|(field, source)| {
                let label = match source {
                    ParamSource::Layer(i) => names.get(i).copied().unwrap_or("?"),
                    ParamSource::Coupled => "coupled",
                    ParamSource::Unset => "unset",
                };
                format!("{field}={label}")
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[cfg(test)]
#[path = "sampling_provenance_tests.rs"]
mod sampling_provenance_tests;
