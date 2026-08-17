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
//! one that fell out of a floor. `0.0` is a number; "`0.0`, from the floor,
//! because the profile claimed the temperature" is an explanation, and only
//! the second makes the behaviour auditable.
//!
//! ## One computation, not two
//!
//! [`FieldSources`] is produced by
//! [`resolve_layers_with_sources`](crate::domain::InferenceConfig::resolve_layers_with_sources),
//! the same pass that decides the values — never by a second function that
//! re-derives the rules. That is deliberate: this provenance previously lived
//! in a separate `describe_provenance` helper in the request pipeline, and the
//! two implementations had already drifted. A ladder where `cli` supplied a
//! `presence_penalty` and a lower layer claimed the `temperature` resolved the
//! penalty from the claiming layer while the log named `cli`.
//!
//! The same `(value, source)` shape
//! [`resolve_context_size_with_source`](crate::server_config::resolve_context_size_with_source)
//! uses, and for the same reason.

use serde::{Deserialize, Serialize};

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
    /// The class floor, because no layer named a value at all.
    Floor,
    /// A layer claimed `temperature` and this parameter is tuned against it,
    /// so no layer beneath was eligible to supply one.
    ///
    /// Distinct from [`Floor`](Self::Floor) in the fact that matters: here a
    /// lower layer may well have named a value and was **deliberately passed
    /// over**. That is what the coupling rule does, and it is the one thing a
    /// bare resolved number can never explain.
    ///
    /// # It does not imply a value was supplied
    ///
    /// The name predates [ADR 0003], when the floor filled all seven
    /// parameters and being passed over always meant landing on a floor value.
    /// Six of those are now deferred to llama.cpp, so a coupled parameter can
    /// resolve to `None`: the rule fired, nothing beneath was eligible, and
    /// the floor had nothing to offer either.
    ///
    /// The variant still reports the rule rather than degrading to
    /// [`Unset`](Self::Unset), because "a value was discarded here" and
    /// "nobody ever named one" are different explanations and only the first
    /// tells a reader where to look. Check the resolved value for whether
    /// anything was ultimately sent.
    ///
    /// [ADR 0003]: https://github.com/mmogr/gglib/blob/main/docs/adr/0003-defer-sampler-defaults-to-llama-cpp.md
    FloorCoupled,
    /// A layer named a value, and a later stage **suppressed** it because the
    /// model's observed template capabilities say the template never reads the
    /// field. Nothing was sent.
    ///
    /// Only `reasoning_effort` can carry this today — it is the one modelled
    /// field delivered by a chat template reading a variable rather than by
    /// the sampler ([ADR 0007] decision 3). The suppressed level and the rung
    /// that supplied it are in the pipeline's own record; see
    /// `request_pipeline::effort_gate::SuppressedEffort`.
    ///
    /// [ADR 0007]: https://github.com/mmogr/gglib/blob/main/docs/adr/0007-ask-the-server-for-template-capabilities.md
    SuppressedByTemplate,
    /// Nothing named it and the class floor carries none either, so no value
    /// is sent and llama.cpp's own default applies. Which fields those are is
    /// whatever
    /// [`InferenceConfig::with_hardcoded_defaults`](crate::domain::InferenceConfig::with_hardcoded_defaults)
    /// leaves unset — deliberately not restated here, because the last
    /// restatement said "`max_tokens` is the only one" and stayed that way
    /// through #741 adding three floorless DRY fields.
    Unset,
}

impl ParamSource {
    /// Whether a person actually chose this value.
    ///
    /// `auto_detected_rung` is the index of the auto-detected per-model rung
    /// in the ladder being asked about — a recipe written at import time is a
    /// guess, not a choice, which is why it already ranks below global
    /// settings.
    ///
    /// # Why this is a method and not a `matches!` at the call site
    ///
    /// It was a `matches!` in `request_pipeline::sampling`, listing the
    /// variants that count as *unchosen*. That shape fails open in the worst
    /// direction: a new `ParamSource` variant is not in the list, so it reads
    /// as "deliberately chosen", and the agentic temperature ceiling silently
    /// stops firing for it. No compile error, no test failure, and the
    /// symptom is a ceiling that quietly does nothing — which is exactly how
    /// #741's floor and #744's ceiling both shipped inert.
    ///
    /// Here the `match` is exhaustive and the arms are the *positive* case,
    /// so a new variant breaks the build at the one place that defines what
    /// "deliberate" means, and whoever adds it has to decide.
    #[must_use]
    pub const fn is_deliberate_choice(self, auto_detected_rung: usize) -> bool {
        match self {
            // A rung someone configured — unless it is the auto-detected
            // recipe, which nobody reviewed.
            Self::Layer(i) => i != auto_detected_rung,
            // Nothing named it, the coupling rule passed over whatever did, or
            // a template gate threw away what a rung chose. None of the three
            // leaves a chosen value standing on *this* parameter.
            Self::Floor | Self::FloorCoupled | Self::Unset | Self::SuppressedByTemplate => false,
        }
    }
}

/// The five rungs of the ladder
/// [`resolve_with_profile`](crate::domain::InferenceConfig::resolve_with_profile)
/// builds, in priority order.
///
/// Only one of [`ModelUserSet`](Self::ModelUserSet) and
/// [`ModelAutoDetected`](Self::ModelAutoDetected) is ever populated for a
/// given model — both name `Model.inference_defaults`, and
/// [`DefaultsOrigin`](crate::domain::DefaultsOrigin) decides which rung it
/// occupies.
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
    /// below global settings. Three origins share this rung by replacement —
    /// the `reasoning`-tag guess, the model author's published recipe, and a
    /// tune sweep's measured winner — and `DefaultsOrigin` says which one is
    /// actually in it; the surfaces that name the rung read that, not this.
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

    /// Short human-readable label, e.g. `per-model defaults (user-set)`.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Request => "request parameters",
            Self::Profile => "profile",
            Self::ModelUserSet => "per-model defaults (user-set)",
            Self::Global => "global settings",
            Self::ModelAutoDetected => "per-model defaults (auto-detected)",
        }
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
                    ParamSource::Floor | ParamSource::FloorCoupled => "floor",
                    ParamSource::Unset => "unset",
                    ParamSource::SuppressedByTemplate => "suppressed-by-template",
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
