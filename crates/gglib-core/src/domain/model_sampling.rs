//! The sampler defaults a model carries in its own GGUF metadata.
//!
//! Inputs come from the raw GGUF key/value map that `gglib-gguf` copies
//! verbatim into [`crate::domain::Model::metadata`] — the same pattern as
//! [`crate::domain::estimate_kv_elems_per_token`] and
//! [`crate::domain::kv_memory_is_partial`], and this module sits beside them
//! for that reason.
//!
//! # Why gglib has to know about these
//!
//! llama.cpp PR #17120 (merged 2025-11-25, and in the pinned build) added
//! `common_init_sampler_from_model`, which overwrites `params.sampling` from
//! the model's own `general.sampling.*` keys **for every field no CLI flag
//! set** — and `GET /props` renders `default_generation_settings` from that
//! same struct.
//!
//! So `/props` answers *"what will this server with this model default to"*,
//! not *"what does this build default to"*. Since [ADR 0003] gglib passes no
//! sampler flags at all, model metadata always wins where it is present.
//!
//! [`gglib_proxy::props`]'s baseline check compares `/props` against a table
//! measured for the pinned build. Without this module it reports a model's own
//! recommendation as *drift* — "this build's default has moved, ADR 0003's
//! deferral is re-opened" — which is a false alarm on the one instrument whose
//! whole value is being worth believing when it fires.
//!
//! # Five keys of twelve
//!
//! llama.cpp reads twelve `general.sampling.*` keys. Only five name a
//! parameter gglib has a floor opinion about, and those are the only ones
//! modelled here — the same rule `SlotParams` states about `/props`'s 42
//! fields: naming the rest would invent an obligation to keep up with them.
//!
//! ```text
//!   gglib field        GGUF key
//!   temperature        general.sampling.temp
//!   top_p              general.sampling.top_p
//!   top_k              general.sampling.top_k
//!   min_p              general.sampling.min_p
//!   repeat_penalty     general.sampling.penalty_repeat
//!
//!   presence_penalty   (none)
//!   dry_multiplier     (none)
//! ```
//!
//! The asymmetry at the bottom is worth stating rather than leaving to be
//! rediscovered: `presence_penalty` and `dry_multiplier` have no GGUF key at
//! all, so they stay attributable to the build whatever a model ships. A
//! baseline check therefore cannot go fully blind on a model's account.
//!
//! The other seven keys — `sequence`, `xtc_probability`, `xtc_threshold`,
//! `penalty_last_n`, `mirostat`, `mirostat_tau`, `mirostat_eta` — move
//! sampling with nothing in gglib watching, because gglib has no floor for
//! them to contradict.
//!
//! # Not architecture-prefixed
//!
//! Unlike its two siblings, which look up `{arch}.{suffix}` and fall back to
//! the bare suffix. `general.sampling.*` is a `general.*` key like
//! `general.architecture`: there is one spelling and a prefixed fallback would
//! match keys that do not exist.
//!
//! [ADR 0003]: https://github.com/mmogr/gglib/blob/main/docs/adr/0003-defer-sampler-defaults-to-llama-cpp.md

use std::collections::HashMap;
use std::hash::BuildHasher;

/// What one model's GGUF says about one sampler field.
///
/// Three states rather than `Option<f64>`, because the two ways of having no
/// number license opposite conclusions. A model that names nothing leaves the
/// build's own default observable in `/props`; a model that names something
/// gglib cannot read leaves nothing observable, because llama.cpp's `strtof`
/// and Rust's `f64::from_str` need not agree on every string and gglib cannot
/// tell from here which of them took the value.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum ModelSamplingDefault {
    /// The GGUF names no value, so llama.cpp's build default applies and
    /// `/props` is a clean read of it.
    #[default]
    Absent,
    /// The GGUF names it. llama.cpp overwrites its own default with this for
    /// every request that does not set the field.
    Declared(f64),
    /// The GGUF names it and gglib could not read the value as a number.
    Unreadable,
}

/// gglib's wire name for a sampler field, paired with the GGUF key that can
/// move it.
///
/// The single mapping table. `gglib_proxy::props::UPSTREAM_DEFAULTS` names the
/// fields the baseline check compares; this names which of them a model can
/// reach, and a test over there asserts the two agree in both directions.
pub const MODEL_SAMPLING_KEYS: [(&str, &str); 5] = [
    ("temperature", "general.sampling.temp"),
    ("top_p", "general.sampling.top_p"),
    ("top_k", "general.sampling.top_k"),
    ("min_p", "general.sampling.min_p"),
    ("repeat_penalty", "general.sampling.penalty_repeat"),
];

/// The sampler defaults one model declares.
///
/// `Copy`, and deliberately so: it rides [`ModelLaunchSpec`] into the resident
/// set and out again on every `current_model()` call, so it is cloned far more
/// often than it is built. Adding a `String` here would put an allocation on
/// the admission fast path and cost `with_model_sampling` its `const`.
///
/// [`ModelLaunchSpec`]: crate::ports::ModelLaunchSpec
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ModelSamplingDefaults {
    /// `general.sampling.temp`.
    pub temperature: ModelSamplingDefault,
    /// `general.sampling.top_p`.
    pub top_p: ModelSamplingDefault,
    /// `general.sampling.top_k`.
    pub top_k: ModelSamplingDefault,
    /// `general.sampling.min_p`.
    pub min_p: ModelSamplingDefault,
    /// `general.sampling.penalty_repeat`.
    pub repeat_penalty: ModelSamplingDefault,
}

impl ModelSamplingDefaults {
    /// Read what a model declares out of its stored GGUF metadata.
    ///
    /// Values arrive stringified by `GgufValue::to_string()`, so a `FLOAT32`
    /// `0.7` is the string `"0.7"` — Rust's `Display` prints the shortest form
    /// that round-trips. Parsing it back yields `0.7`, while `/props` reports
    /// the same value widened from `f32` (`0.699999988079071`); the ~1.2e-8 gap
    /// is well inside the epsilon the comparison uses.
    #[must_use]
    pub fn from_metadata<S: BuildHasher>(metadata: &HashMap<String, String, S>) -> Self {
        let read = |key: &str| {
            metadata
                .get(key)
                .map_or(ModelSamplingDefault::Absent, |raw| {
                    raw.trim()
                        .parse::<f64>()
                        .map_or(ModelSamplingDefault::Unreadable, |v| {
                            ModelSamplingDefault::Declared(v)
                        })
                })
        };
        Self {
            temperature: read("general.sampling.temp"),
            top_p: read("general.sampling.top_p"),
            top_k: read("general.sampling.top_k"),
            min_p: read("general.sampling.min_p"),
            repeat_penalty: read("general.sampling.penalty_repeat"),
        }
    }

    /// Look one field up by its gglib wire name.
    ///
    /// Mirrors `SlotParams::get` so the baseline check reads both sides the
    /// same way. A name with no GGUF key — `presence_penalty`,
    /// `dry_multiplier`, or anything unknown — is [`Absent`] **by
    /// construction**, not because this model happened not to set it.
    ///
    /// [`Absent`]: ModelSamplingDefault::Absent
    #[must_use]
    pub fn get(&self, field: &str) -> ModelSamplingDefault {
        match field {
            "temperature" => self.temperature,
            "top_p" => self.top_p,
            "top_k" => self.top_k,
            "min_p" => self.min_p,
            "repeat_penalty" => self.repeat_penalty,
            _ => ModelSamplingDefault::Absent,
        }
    }

    /// The GGUF key that can move `field`, if any.
    #[must_use]
    pub fn gguf_key(field: &str) -> Option<&'static str> {
        MODEL_SAMPLING_KEYS
            .iter()
            .find(|(name, _)| *name == field)
            .map(|(_, key)| *key)
    }
}

// =============================================================================
// What gglib does with what the model published
// =============================================================================

/// Tolerance for comparing a published value against a resolved one.
///
/// Same value and reason as `gglib_proxy::props`'s. A GGUF `FLOAT32` `0.7`
/// stringifies to `"0.7"` and parses back to `f64` `0.7`, while gglib's own
/// resolved `f32` `0.7` widens to `0.699999988079071`. The ~1.2e-8 gap is an
/// artefact of the round trip, not a disagreement, and must not render as one.
const FLOAT_EPSILON: f64 = 1e-6;

/// What gglib is doing with one field's published recommendation.
///
/// # Why this is a configuration question, not an observation
///
/// [`gglib_proxy::props`]'s baseline check asks *"has this build's default
/// table moved?"* and answers it from `/props`. This asks a different question
/// with a different failure mode: *"is gglib sending something other than what
/// the model author published?"* — which is decidable from stored
/// configuration alone, with no server running and no request in flight.
///
/// Keeping them apart matters. The baseline check must abstain wherever
/// attribution fails, because a wrong verdict there re-opens or falsely
/// satisfies [ADR 0003]'s deletion criterion. This comparison has no such
/// hazard: both sides are known exactly, so every field reaches a verdict and
/// none of them is `Indeterminate`.
///
/// # The wire rule this encodes
///
/// A sampling value gglib resolves is sent in the request body, and the body
/// wins over `default_generation_settings`. A value gglib leaves unresolved is
/// sent as nothing at all, and llama.cpp then applies the model's own
/// `general.sampling.*` key ([ADR 0004] finding 7). So the question "does the
/// model's published value survive to the sampler?" is answered entirely by
/// **whether gglib names the field**, not by which rung named it — which is
/// why this keys on `Option<f64>` rather than on [`ParamSource`].
///
/// [`gglib_proxy::props`]: https://github.com/mmogr/gglib/blob/main/crates/gglib-proxy/src/props.rs
/// [`ParamSource`]: crate::domain::ParamSource
/// [ADR 0003]: https://github.com/mmogr/gglib/blob/main/docs/adr/0003-defer-sampler-defaults-to-llama-cpp.md
/// [ADR 0004]: https://github.com/mmogr/gglib/blob/main/docs/adr/0004-observe-the-sampling-boundary.md
#[derive(Debug, Clone, PartialEq)]
pub enum SamplingOverride {
    /// The model published nothing gglib could act against — either it names
    /// no value for this field, or the field has no GGUF key at all.
    ///
    /// The two are collapsed deliberately: both mean *there is no published
    /// recommendation to override*, which is the only thing a surface needs in
    /// order to stay quiet. [`ModelSamplingDefaults::gguf_key`] tells the two
    /// apart where it matters.
    NotPublished,
    /// The model published a value and gglib names nothing, so llama.cpp
    /// applies the model author's number.
    ///
    /// This is the state [ADR 0003]'s deferral was aiming at, and the one a
    /// bare `—` in an explain table renders indistinguishably from a gap.
    ///
    /// [ADR 0003]: https://github.com/mmogr/gglib/blob/main/docs/adr/0003-defer-sampler-defaults-to-llama-cpp.md
    Deferred {
        /// The GGUF key carrying it.
        key: &'static str,
        /// What the model author published.
        published: f64,
    },
    /// The model published a value and gglib sends the same number.
    ///
    /// Not an override in effect, and reporting it as one would cry wolf. Kept
    /// distinct from [`Self::Deferred`] anyway, because gglib *asserting* a
    /// value it happens to agree with is exactly the redundant restatement
    /// [ADR 0003] argues against — it silently overrides whatever the model
    /// author chooses next.
    ///
    /// [ADR 0003]: https://github.com/mmogr/gglib/blob/main/docs/adr/0003-defer-sampler-defaults-to-llama-cpp.md
    Restated {
        /// The GGUF key carrying it.
        key: &'static str,
        /// The value both sides name.
        published: f64,
    },
    /// The model published a value and gglib sends a different one.
    ///
    /// The state this whole comparison exists to surface.
    Overridden {
        /// The GGUF key carrying it.
        key: &'static str,
        /// What the model author published.
        published: f64,
        /// What gglib puts on the wire instead.
        sending: f64,
    },
    /// The model names the key and gglib could not read its value.
    ///
    /// Carried through rather than folded into [`Self::NotPublished`] for the
    /// reason [`ModelSamplingDefault::Unreadable`] exists: llama.cpp's `strtof`
    /// and Rust's `f64::from_str` need not agree on every string, so gglib
    /// cannot say whether a recommendation was applied here or not.
    Unreadable {
        /// The GGUF key whose value could not be read.
        key: &'static str,
        /// What gglib sends regardless, if anything. Its own value still
        /// reaches the sampler; what is unknown is what it displaced.
        sending: Option<f64>,
    },
}

impl SamplingOverride {
    /// Whether gglib is putting a different number on the wire than the model
    /// author published.
    ///
    /// The one predicate a surface should branch on to decide whether to warn.
    /// [`Self::Unreadable`] is deliberately **not** included: gglib cannot tell
    /// whether it is overriding anything there, and a warning that might be
    /// about nothing is the [`Indeterminate`]-rendered-as-`Differs` mistake
    /// [ADR 0004] decision 3 forbids one layer up.
    ///
    /// [`Indeterminate`]: https://github.com/mmogr/gglib/blob/main/crates/gglib-proxy/src/props.rs
    /// [ADR 0004]: https://github.com/mmogr/gglib/blob/main/docs/adr/0004-observe-the-sampling-boundary.md
    #[must_use]
    pub const fn is_override(&self) -> bool {
        matches!(self, Self::Overridden { .. })
    }

    /// Whether the model published anything at all for this field.
    #[must_use]
    pub const fn model_published(&self) -> bool {
        !matches!(self, Self::NotPublished)
    }
}

impl ModelSamplingDefaults {
    /// Compare one field's published value against what gglib will send.
    ///
    /// `sending` is what the ladder resolved: `None` means gglib names the
    /// field nowhere and llama.cpp is left to apply the model's own value.
    #[must_use]
    pub fn compare_field(&self, field: &str, sending: Option<f64>) -> SamplingOverride {
        let Some(key) = Self::gguf_key(field) else {
            // No GGUF key exists, so no model can have published one. This is
            // the `presence_penalty` / `dry_multiplier` arm, and it is a fact
            // about the format rather than about this model.
            return SamplingOverride::NotPublished;
        };
        match self.get(field) {
            ModelSamplingDefault::Absent => SamplingOverride::NotPublished,
            ModelSamplingDefault::Unreadable => SamplingOverride::Unreadable { key, sending },
            ModelSamplingDefault::Declared(published) => match sending {
                None => SamplingOverride::Deferred { key, published },
                Some(sending) if (sending - published).abs() <= FLOAT_EPSILON => {
                    SamplingOverride::Restated { key, published }
                }
                Some(sending) => SamplingOverride::Overridden {
                    key,
                    published,
                    sending,
                },
            },
        }
    }

    /// Compare every field a model can publish, in [`MODEL_SAMPLING_KEYS`]
    /// order.
    ///
    /// `resolved` looks each field up by its gglib wire name and returns what
    /// the ladder decided to send, so callers hand in whichever config they are
    /// explaining rather than this module learning about [`InferenceConfig`].
    ///
    /// [`InferenceConfig`]: crate::domain::InferenceConfig
    #[must_use]
    pub fn compare_all(
        &self,
        resolved: impl Fn(&str) -> Option<f64>,
    ) -> Vec<(&'static str, SamplingOverride)> {
        MODEL_SAMPLING_KEYS
            .iter()
            .map(|(field, _)| (*field, self.compare_field(field, resolved(field))))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    /// The ordinary case, and the one that keeps today's behaviour: a model
    /// with no opinion leaves the build's own defaults observable.
    #[test]
    fn a_gguf_that_names_no_sampler_defaults_declares_nothing() {
        let d = ModelSamplingDefaults::from_metadata(&meta(&[
            ("general.architecture", "qwen3"),
            ("qwen3.block_count", "36"),
        ]));

        assert_eq!(d, ModelSamplingDefaults::default());
        assert_eq!(d.get("temperature"), ModelSamplingDefault::Absent);
    }

    #[test]
    fn a_model_embedded_temperature_is_read_from_general_sampling_temp() {
        let d = ModelSamplingDefaults::from_metadata(&meta(&[("general.sampling.temp", "0.33")]));

        assert_eq!(d.temperature, ModelSamplingDefault::Declared(0.33));
        assert_eq!(d.get("temperature"), ModelSamplingDefault::Declared(0.33));
        assert_eq!(d.top_p, ModelSamplingDefault::Absent, "others untouched");
    }

    /// The one key whose GGUF name differs from gglib's wire name. A typo here
    /// would be silent: the field would simply always read as `Absent`.
    #[test]
    fn repeat_penalty_is_read_from_general_sampling_penalty_repeat() {
        let d = ModelSamplingDefaults::from_metadata(&meta(&[(
            "general.sampling.penalty_repeat",
            "1.07",
        )]));

        assert_eq!(
            d.get("repeat_penalty"),
            ModelSamplingDefault::Declared(1.07)
        );
        assert_eq!(
            ModelSamplingDefaults::gguf_key("repeat_penalty"),
            Some("general.sampling.penalty_repeat")
        );
    }

    /// "Named but unreadable" is not "not named". The first means llama.cpp
    /// may have applied something gglib cannot see; the second means the build
    /// default stands and `/props` can be trusted for that field.
    #[test]
    fn a_value_that_is_not_a_number_is_unreadable_rather_than_absent() {
        let d = ModelSamplingDefaults::from_metadata(&meta(&[("general.sampling.temp", "warm")]));

        assert_eq!(d.temperature, ModelSamplingDefault::Unreadable);
        assert_ne!(d.temperature, ModelSamplingDefault::Absent);
    }

    /// Unlike `kv_estimate` and `kv_memory`, whose keys are `{arch}.{suffix}`.
    /// A prefixed lookup here would match keys that do not exist and miss the
    /// one that does.
    #[test]
    fn the_sampling_keys_are_not_architecture_prefixed() {
        let d = ModelSamplingDefaults::from_metadata(&meta(&[
            ("general.architecture", "qwen3"),
            ("qwen3.sampling.temp", "0.33"),
            ("sampling.temp", "0.44"),
        ]));

        assert_eq!(
            d.temperature,
            ModelSamplingDefault::Absent,
            "only general.sampling.temp counts"
        );
    }

    /// **The asymmetry that makes the baseline check still worth running.**
    /// These two have no GGUF key, so no model can move them and the build
    /// stays observable through them however much else it declares.
    #[test]
    fn presence_penalty_and_dry_multiplier_have_no_gguf_key() {
        // Invented keys a model author might plausibly try.
        let d = ModelSamplingDefaults::from_metadata(&meta(&[
            ("general.sampling.presence_penalty", "1.0"),
            ("general.sampling.dry_multiplier", "0.8"),
        ]));

        for field in ["presence_penalty", "dry_multiplier"] {
            assert_eq!(
                ModelSamplingDefaults::gguf_key(field),
                None,
                "{field} must have no GGUF key"
            );
            assert_eq!(
                d.get(field),
                ModelSamplingDefault::Absent,
                "{field} must be unreachable by a model, not merely unset"
            );
        }
    }

    #[test]
    fn whitespace_around_a_declared_value_is_tolerated() {
        let d =
            ModelSamplingDefaults::from_metadata(&meta(&[("general.sampling.top_p", " 0.71 ")]));
        assert_eq!(d.top_p, ModelSamplingDefault::Declared(0.71));
    }

    /// `top_k` is an integer in the GGUF and stringifies without a decimal
    /// point; reading everything as `f64` keeps one comparison path.
    #[test]
    fn an_integer_valued_key_reads_as_a_float() {
        let d = ModelSamplingDefaults::from_metadata(&meta(&[("general.sampling.top_k", "17")]));
        assert_eq!(d.top_k, ModelSamplingDefault::Declared(17.0));
    }

    #[test]
    fn an_unknown_field_name_is_absent_rather_than_a_panic() {
        let d = ModelSamplingDefaults::default();
        assert_eq!(d.get("mirostat"), ModelSamplingDefault::Absent);
        assert_eq!(ModelSamplingDefaults::gguf_key("mirostat"), None);
    }

    // =========================================================================
    // The override comparison
    // =========================================================================

    /// The state the whole comparison exists to surface: a model author
    /// published a number and gglib puts a different one on the wire.
    #[test]
    fn a_resolved_value_that_differs_from_the_published_one_is_an_override() {
        let d = ModelSamplingDefaults::from_metadata(&meta(&[("general.sampling.temp", "0.33")]));

        let verdict = d.compare_field("temperature", Some(1.0));

        assert_eq!(
            verdict,
            SamplingOverride::Overridden {
                key: "general.sampling.temp",
                published: 0.33,
                sending: 1.0,
            }
        );
        assert!(verdict.is_override());
    }

    /// gglib naming nothing is what lets the model's own value through. This
    /// must never read as an override, and it is the state a bare `—` in an
    /// explain table cannot distinguish from a gap.
    #[test]
    fn naming_nothing_defers_to_the_published_value() {
        let d = ModelSamplingDefaults::from_metadata(&meta(&[("general.sampling.top_p", "0.71")]));

        let verdict = d.compare_field("top_p", None);

        assert_eq!(
            verdict,
            SamplingOverride::Deferred {
                key: "general.sampling.top_p",
                published: 0.71,
            }
        );
        assert!(!verdict.is_override());
        assert!(verdict.model_published());
    }

    /// Sending the same number is not an override in effect, and warning about
    /// it would cry wolf — but it is not deferral either, so it keeps its own
    /// arm.
    #[test]
    fn sending_the_published_value_is_restated_rather_than_overridden() {
        let d = ModelSamplingDefaults::from_metadata(&meta(&[("general.sampling.min_p", "0.05")]));

        let verdict = d.compare_field("min_p", Some(0.05));

        assert_eq!(
            verdict,
            SamplingOverride::Restated {
                key: "general.sampling.min_p",
                published: 0.05,
            }
        );
        assert!(!verdict.is_override());
    }

    /// **The round-trip guard.** A GGUF `FLOAT32` `0.7` reaches this module as
    /// the string `"0.7"` and parses to `f64` `0.7`, while gglib's own resolved
    /// `f32` `0.7` widens to `0.699999988079071`. Comparing those exactly would
    /// report an override on every model that publishes a value gglib agrees
    /// with — the loudest possible false alarm.
    #[test]
    fn an_f32_round_trip_does_not_read_as_an_override() {
        let d = ModelSamplingDefaults::from_metadata(&meta(&[("general.sampling.temp", "0.7")]));

        let widened = f64::from(0.7_f32);
        assert!(
            (widened - 0.7).abs() > f64::EPSILON,
            "guards the premise: the gap is real, not an artefact of this assertion"
        );

        assert!(matches!(
            d.compare_field("temperature", Some(widened)),
            SamplingOverride::Restated { .. }
        ));
    }

    /// A difference larger than the epsilon still has to register, or the
    /// tolerance above would have silenced the check rather than calibrated it.
    #[test]
    fn a_difference_above_the_epsilon_still_registers() {
        let d = ModelSamplingDefaults::from_metadata(&meta(&[("general.sampling.temp", "0.7")]));

        assert!(d.compare_field("temperature", Some(0.7001)).is_override());
    }

    /// gglib cannot tell whether it displaced anything here, so this must not
    /// claim an override — the same rule ADR 0004 decision 3 applies to
    /// `Indeterminate` one layer up.
    #[test]
    fn an_unreadable_published_value_is_not_reported_as_an_override() {
        let d = ModelSamplingDefaults::from_metadata(&meta(&[("general.sampling.temp", "warm")]));

        let verdict = d.compare_field("temperature", Some(1.0));

        assert_eq!(
            verdict,
            SamplingOverride::Unreadable {
                key: "general.sampling.temp",
                sending: Some(1.0),
            }
        );
        assert!(!verdict.is_override(), "cannot claim what it cannot know");
        assert!(verdict.model_published());
    }

    /// A model with no opinion leaves gglib free, and the surfaces silent.
    #[test]
    fn a_field_the_model_never_named_is_not_published() {
        let d = ModelSamplingDefaults::default();

        let verdict = d.compare_field("temperature", Some(1.0));

        assert_eq!(verdict, SamplingOverride::NotPublished);
        assert!(!verdict.is_override());
        assert!(!verdict.model_published());
    }

    /// **The asymmetry, restated at the comparison layer.** These two have no
    /// GGUF key, so gglib naming them can never be overriding a model author —
    /// and a surface must not imply otherwise however loudly the model declares
    /// keys with those names.
    #[test]
    fn a_field_with_no_gguf_key_can_never_be_an_override() {
        let d = ModelSamplingDefaults::from_metadata(&meta(&[
            ("general.sampling.presence_penalty", "1.0"),
            ("general.sampling.dry_multiplier", "0.8"),
        ]));

        for field in ["presence_penalty", "dry_multiplier"] {
            assert_eq!(
                d.compare_field(field, Some(1.5)),
                SamplingOverride::NotPublished,
                "{field} is unreachable by a model"
            );
        }
    }

    /// `compare_all` covers exactly the reachable set, in the mapping table's
    /// order, so a surface iterating it cannot silently miss a field.
    #[test]
    fn compare_all_covers_every_reachable_field_in_table_order() {
        let d = ModelSamplingDefaults::from_metadata(&meta(&[
            ("general.sampling.temp", "0.33"),
            ("general.sampling.penalty_repeat", "1.07"),
        ]));

        let all = d.compare_all(|field| match field {
            "temperature" => Some(1.0),
            "repeat_penalty" => Some(1.07),
            _ => None,
        });

        let fields: Vec<&str> = all.iter().map(|(f, _)| *f).collect();
        assert_eq!(
            fields,
            MODEL_SAMPLING_KEYS
                .iter()
                .map(|(f, _)| *f)
                .collect::<Vec<_>>()
        );

        let by_field = |name: &str| {
            all.iter()
                .find(|(f, _)| *f == name)
                .map(|(_, v)| v.clone())
                .expect("field present")
        };
        assert!(by_field("temperature").is_override());
        assert!(matches!(
            by_field("repeat_penalty"),
            SamplingOverride::Restated { .. }
        ));
        assert_eq!(by_field("top_p"), SamplingOverride::NotPublished);
    }
}
