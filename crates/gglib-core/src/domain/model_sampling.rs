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

    /// Whether this model declares anything at all.
    #[must_use]
    pub fn declares_anything(&self) -> bool {
        MODEL_SAMPLING_KEYS
            .iter()
            .any(|(name, _)| !matches!(self.get(name), ModelSamplingDefault::Absent))
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
        assert!(!d.declares_anything());
        assert_eq!(d.get("temperature"), ModelSamplingDefault::Absent);
    }

    #[test]
    fn a_model_embedded_temperature_is_read_from_general_sampling_temp() {
        let d = ModelSamplingDefaults::from_metadata(&meta(&[("general.sampling.temp", "0.33")]));

        assert_eq!(d.temperature, ModelSamplingDefault::Declared(0.33));
        assert_eq!(d.get("temperature"), ModelSamplingDefault::Declared(0.33));
        assert!(d.declares_anything());
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
        assert!(d.declares_anything());
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
        assert!(!d.declares_anything());
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
}
