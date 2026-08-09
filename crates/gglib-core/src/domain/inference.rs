//! Inference configuration types.
//!
//! Defines shared types for configuring LLM inference parameters
//! (temperature, `top_p`, `top_k`, `max_tokens`, `repeat_penalty`,
//! `presence_penalty`, `min_p`).
//!
//! **Tier B — Policy** ([ADR 0001]) for the hierarchy: the ordered fold,
//! profiles, the user-set versus auto-detected split and the class floors are
//! decisions llama-server is structurally not in a position to make, so
//! nothing here gates on [`RuntimeCapabilities`].
//!
//! [`with_hardcoded_defaults`](InferenceConfig::with_hardcoded_defaults) is
//! the exception, and it is the same shape as ADR 0001's `truncation` caveat:
//! the *policy* of having a floor is gglib's, but a floor *value* that equals
//! llama.cpp's own default is a redundant assertion rather than a decision.
//! Six of the seven were measured to be exactly that. [ADR 0003] decides they
//! are deferred, leaving `temperature` — the one genuine divergence, 0.7
//! against upstream's 0.8 — plus
//! [`reasoning_floor`](InferenceConfig::reasoning_floor)'s class-aware
//! overrides.
//!
//! [ADR 0001]: https://github.com/mmogr/gglib/blob/main/docs/adr/0001-runtime-capability-tiers.md
//! [ADR 0003]: https://github.com/mmogr/gglib/blob/main/docs/adr/0003-defer-sampler-defaults-to-llama-cpp.md
//! [`RuntimeCapabilities`]: crate::domain::RuntimeCapabilities
//!
//! This module provides the core `InferenceConfig` type that is reused across:
//! - Per-model defaults (`Model.inference_defaults`)
//! - Global settings (`Settings.inference_defaults`)
//! - Request-level overrides (flattened in `ChatProxyRequest`)
//! - `gglib proxy` — per-request injection into OpenAI-format request bodies
//! - `gglib chat` / `gglib q` — hierarchy resolution for the agentic loop
//!
//! All surfaces resolve inference parameters through
//! [`InferenceConfig::resolve_with_profile`], which is the single source of
//! truth for the hierarchy. [`InferenceConfig::resolve_with_defaults`] is the
//! same resolution with no profile selected, for surfaces that have no notion
//! of one.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::domain::sampling_provenance::{FieldSources, ParamSource};

/// Inference parameters for LLM sampling.
///
/// All fields are optional to support partial configuration and fallback chains.
/// Intended to be shared across model defaults, global settings, and request overrides.
///
/// # Hierarchy Resolution
///
/// When making an inference request, parameters are resolved in this order:
/// 1. Request-level override (user specified for this request)
/// 2. Selected profile (`Settings.inference_profiles`, chosen as
///    `{model}:{profile}`; absent on surfaces without profiles)
/// 3. Per-model defaults (stored in `Model.inference_defaults`)
/// 4. Global settings (stored in `Settings.inference_defaults`)
/// 5. Hardcoded fallback (e.g., temperature = 0.7)
///
/// # Examples
///
/// ```rust
/// use gglib_core::domain::InferenceConfig;
///
/// // Conservative settings for code generation
/// let code_gen = InferenceConfig {
///     temperature: Some(0.2),
///     top_p: Some(0.9),
///     top_k: Some(40),
///     max_tokens: Some(2048),
///     repeat_penalty: Some(1.1),
///     ..Default::default()
/// };
///
/// // Creative writing settings
/// let creative = InferenceConfig {
///     temperature: Some(1.2),
///     top_p: Some(0.95),
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct InferenceConfig {
    /// Sampling temperature (0.0 - 2.0).
    ///
    /// Controls randomness in token selection:
    /// - Lower values (0.1-0.5): More deterministic, focused
    /// - Medium values (0.7-1.0): Balanced creativity
    /// - Higher values (1.1-2.0): More random, creative
    pub temperature: Option<f32>,

    /// Nucleus sampling threshold (0.0 - 1.0).
    ///
    /// Considers only the top tokens whose cumulative probability exceeds this threshold.
    /// Common values: 0.9 (default), 0.95 (more diverse)
    pub top_p: Option<f32>,

    /// Top-K sampling limit.
    ///
    /// Considers only the K most likely next tokens.
    /// Common values: 40 (default), 10 (focused), 100 (diverse)
    pub top_k: Option<i32>,

    /// Maximum tokens to generate in response.
    ///
    /// Hard limit on response length. Does not include input tokens.
    pub max_tokens: Option<u32>,

    /// Repetition penalty (> 0.0, typically 1.0 - 1.3).
    ///
    /// Penalizes repeated tokens to reduce repetitive output.
    /// - 1.0: No penalty (default)
    /// - 1.1-1.3: Moderate penalty
    /// - > 1.3: Strong penalty (may hurt coherence)
    pub repeat_penalty: Option<f32>,

    /// Presence penalty (0.0 - 2.0).
    ///
    /// Penalizes tokens that have already appeared in the output, encouraging
    /// the model to cover new ground. Effective at preventing repetitive
    /// reasoning loops in thinking models.
    /// - 0.0: No penalty (default; disabled)
    /// - 1.5: Recommended for reasoning/thinking models (e.g. `Qwen3.6`, `DeepSeek-R1`)
    /// - > 2.0: Avoid; may degrade coherence
    pub presence_penalty: Option<f32>,

    /// Minimum-probability sampling threshold (0.0 - 1.0).
    ///
    /// Removes tokens whose probability is below `min_p × P(top token)`.
    /// - 0.0: Disabled (explicit off; recommended by Qwen3.6, and the floor
    ///   for `reasoning`-tagged models — see [`reasoning_floor`])
    /// - 0.05: llama.cpp's own default, and the neutral floor here — see
    ///   [`with_hardcoded_defaults`]
    ///
    /// [`reasoning_floor`]: Self::reasoning_floor
    /// [`with_hardcoded_defaults`]: Self::with_hardcoded_defaults
    pub min_p: Option<f32>,

    /// DRY (Don't Repeat Yourself) penalty strength.
    ///
    /// Penalises tokens that would extend a sequence already present in the
    /// context, which catches the multi-token degenerate loops that
    /// `repeat_penalty` — a flat per-token penalty — cannot see.
    /// - 0.0: Disabled (llama.cpp's default, and the floor here)
    /// - 0.8: A common starting point for long agentic sessions
    ///
    /// Left alone on agentic turns, deliberately. An earlier version forced
    /// this to `0` whenever a request carried tools, reasoning that structured
    /// output legitimately repeats tokens. Both halves were wrong: llama.cpp's
    /// sequence breakers already default to `\n`, `:`, `"`, `*` — two of which
    /// are pervasive in JSON — and agentic clients send `tools` on *every*
    /// request, so the pin would have disabled DRY for whole sessions, which
    /// is the workload it exists for.
    ///
    /// llama.cpp's fifth DRY parameter, `--dry-sequence-breaker`, is not
    /// modelled: it is a list of strings, and every layer of this hierarchy —
    /// merge, coupling, the CLI flags, the settings mirror — is built for
    /// scalars. It is also the right lever if DRY is ever seen mangling a tool
    /// call, so modelling it is the follow-up, not switching DRY off.
    pub dry_multiplier: Option<f32>,

    /// DRY penalty base, the exponent applied per token of matched sequence
    /// length. Higher grows the penalty faster on longer repeats.
    /// Unset defers to llama.cpp's own default (1.75).
    pub dry_base: Option<f32>,

    /// Sequence length, in tokens, that DRY tolerates before penalising.
    /// Unset defers to llama.cpp's own default (2).
    pub dry_allowed_length: Option<i32>,

    /// How far back DRY scans for repeats, in tokens. `0` disables the
    /// penalty; llama.cpp resolves negative values against the context size.
    /// Unset defers to llama.cpp's own default (64).
    pub dry_penalty_last_n: Option<i32>,
}

/// Whether a model's stored `inference_defaults` were set by the user or
/// written automatically at import time.
///
/// `Model.inference_defaults` is populated two ways: a user explicitly
/// tunes it (`gglib model update --presence-penalty …`, or the `WebUI` edit
/// form), or [`crate::services`]'s import path auto-writes
/// [`InferenceConfig::reasoning_profile`] onto any model carrying the
/// `reasoning` tag — a reasonable guess, not a user decision. Both end up in
/// the same column with nothing distinguishing them, which meant an
/// auto-written guess silently outranked the user's own global settings in
/// the resolution ladder ([`InferenceConfig::resolve_with_profile`]) exactly
/// as if the user had tuned it themselves.
///
/// This type tracks which one actually happened, so resolution can rank
/// [`AutoDetected`](Self::AutoDetected) below global settings while a real
/// [`User`](Self::User) choice keeps outranking them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DefaultsOrigin {
    /// Set explicitly by the user — a CLI flag, a model-update request, or a
    /// `WebUI` edit. Outranks global settings, same as before this type
    /// existed.
    User,
    /// Written automatically at import time from the model's `reasoning`
    /// tag, never reviewed by a person. Ranks below global settings.
    AutoDetected,
}

impl std::fmt::Display for DefaultsOrigin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::User => write!(f, "user"),
            Self::AutoDetected => write!(f, "auto_detected"),
        }
    }
}

impl std::str::FromStr for DefaultsOrigin {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "user" => Ok(Self::User),
            "auto_detected" => Ok(Self::AutoDetected),
            other => Err(format!(
                "unknown defaults origin '{other}'; expected user or auto_detected"
            )),
        }
    }
}

/// Everything about the target model that changes how sampling resolves,
/// independent of any specific request.
///
/// Bundled rather than passed as separate parameters because both
/// [`InferenceConfig::resolve_with_profile`] and
/// [`crate::request_pipeline::sampling::resolve_sampling`] need the same two
/// facts about the same model, and the list has already grown once (see
/// #685) — a named struct reads at call sites instead of two easily
/// transposed booleans.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ModelSamplingContext {
    /// Whether the model carries gglib's `reasoning` capability tag. Selects
    /// the coupled-trio floor — see [`InferenceConfig::reasoning_floor`].
    pub is_reasoning: bool,
    /// Whether the model's `inference_defaults` were user-set or
    /// auto-detected. `None` when the model has no stored
    /// `inference_defaults` at all, in which case it has no effect either
    /// way. See [`DefaultsOrigin`].
    pub defaults_origin: Option<DefaultsOrigin>,
}

impl ModelSamplingContext {
    /// Read both facts off a catalog row.
    ///
    /// Four call sites across three crates built this struct field-by-field
    /// from a [`Model`](crate::domain::Model), each re-deriving `is_reasoning`
    /// from the tag list inline. Two fields is exactly the size at which
    /// hand-construction looks harmless and stops being so: the pair travels
    /// together, both are read by the same fold, and a call site that filled
    /// one and defaulted the other would resolve against the wrong floor or
    /// mis-rank the model's own defaults against global settings — silently,
    /// in both cases.
    #[must_use]
    pub fn for_model(model: &crate::domain::Model) -> Self {
        Self {
            is_reasoning: crate::domain::capability_tags::is_reasoning(&model.tags),
            defaults_origin: model.defaults_origin,
        }
    }
}

/// Convert a camelCase string to `snake_case`.
///
/// Used internally to rename `InferenceConfig`'s serde camelCase output to the
/// `OpenAI` wire format (`topP` → `top_p`, `maxTokens` → `max_tokens`, etc.).
fn camel_to_snake(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for ch in s.chars() {
        if ch.is_uppercase() {
            out.push('_');
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

/// What reading one client-supplied sampling field did, when it was not
/// simply "read it".
///
/// Carried out of [`InferenceConfig::extract_client_sampling`] so the caller
/// can log or count it. A value gglib declines to use is a fact about a
/// client worth surfacing — before this existed, the entire client sampling
/// layer could vanish over one key with nothing recording that it had.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldIssue {
    /// Recognised, but not in the form this field takes. A documented
    /// equivalent was substituted and the value is in use.
    Normalised {
        /// Wire key, as the client spelled it.
        field: &'static str,
        /// What arrived, rendered for a log line.
        from: String,
        /// What it was taken to mean.
        to: &'static str,
    },
    /// Not readable as this field's type. **This field alone** is dropped;
    /// every other field the client sent is unaffected.
    Rejected {
        /// Wire key, as the client spelled it.
        field: &'static str,
        /// What arrived, rendered for a log line.
        value: String,
        /// What the field accepts.
        expected: &'static str,
    },
}

impl fmt::Display for FieldIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Normalised { field, from, to } => {
                write!(f, "{field}={from} read as {to}")
            }
            Self::Rejected {
                field,
                value,
                expected,
            } => write!(f, "{field}={value} dropped (expected {expected})"),
        }
    }
}

/// Render a JSON value compactly enough for a log line.
fn brief(v: &serde_json::Value) -> String {
    let s = v.to_string();
    if s.len() > 40 {
        format!("{}…", &s[..40])
    } else {
        s
    }
}

/// Narrow a JSON number to the `f32` every sampling field stores.
///
/// Named rather than inline because an `#[allow]` cannot sit on an
/// expression, and the truncation is deliberate: the wire carries `f64` and
/// `InferenceConfig` has always been `f32`.
#[allow(clippy::cast_possible_truncation)]
const fn narrow(n: f64) -> f32 {
    n as f32
}

/// Read one float field. Absent and `null` are both "no opinion".
fn read_f32(
    obj: &serde_json::Map<String, serde_json::Value>,
    key: &'static str,
    issues: &mut Vec<FieldIssue>,
) -> Option<f32> {
    let v = obj.get(key)?;
    if v.is_null() {
        return None;
    }
    v.as_f64().map_or_else(
        || {
            issues.push(FieldIssue::Rejected {
                field: key,
                value: brief(v),
                expected: "a number",
            });
            None
        },
        |n| Some(narrow(n)),
    )
}

/// Read one integer field.
///
/// A float with no fractional part is accepted, because llama.cpp accepts
/// `top_k: 40.0` and several clients emit every number as a float. A float
/// that would lose information is not.
fn read_i32(
    obj: &serde_json::Map<String, serde_json::Value>,
    key: &'static str,
    issues: &mut Vec<FieldIssue>,
) -> Option<i32> {
    let v = obj.get(key)?;
    if v.is_null() {
        return None;
    }
    if let Some(n) = v.as_i64() {
        return i32::try_from(n).map_or_else(
            |_| {
                issues.push(FieldIssue::Rejected {
                    field: key,
                    value: brief(v),
                    expected: "a 32-bit integer",
                });
                None
            },
            Some,
        );
    }
    if let Some(f) = v.as_f64()
        && f.fract() == 0.0
        && f >= f64::from(i32::MIN)
        && f <= f64::from(i32::MAX)
    {
        #[allow(clippy::cast_possible_truncation)]
        let n = f as i32;
        issues.push(FieldIssue::Normalised {
            field: key,
            from: brief(v),
            to: "an integer",
        });
        return Some(n);
    }
    issues.push(FieldIssue::Rejected {
        field: key,
        value: brief(v),
        expected: "an integer",
    });
    None
}

/// Read `max_tokens`, which is `u32` internally and `-1` on the wire.
///
/// `-1` is llama.cpp's own idiom for "no limit", and omitting the key means
/// exactly that here — see
/// [`with_hardcoded_defaults`](InferenceConfig::with_hardcoded_defaults),
/// which deliberately leaves `max_tokens` unset. So `-1` is not an error to
/// be reported, it is a spelling of a value this type already has. Any other
/// negative is a client bug.
fn read_max_tokens(
    obj: &serde_json::Map<String, serde_json::Value>,
    issues: &mut Vec<FieldIssue>,
) -> Option<u32> {
    let v = obj.get("max_tokens")?;
    if v.is_null() {
        return None;
    }
    // Not `read_i32_raw(v)?` — `?` would return before recording anything,
    // which is the same silent-drop shape this whole function exists to end.
    let Some(n) = read_i32_raw(v) else {
        issues.push(FieldIssue::Rejected {
            field: "max_tokens",
            value: brief(v),
            expected: "a non-negative integer, or -1 for no limit",
        });
        return None;
    };
    if n == -1 {
        issues.push(FieldIssue::Normalised {
            field: "max_tokens",
            from: "-1".to_string(),
            to: "no limit",
        });
        return None;
    }
    u32::try_from(n).map_or_else(
        |_| {
            issues.push(FieldIssue::Rejected {
                field: "max_tokens",
                value: brief(v),
                expected: "a non-negative integer, or -1 for no limit",
            });
            None
        },
        Some,
    )
}

/// The integer read behind [`read_max_tokens`], without issue reporting —
/// its caller reports in terms of `max_tokens`' own accepted range.
fn read_i32_raw(v: &serde_json::Value) -> Option<i32> {
    if let Some(n) = v.as_i64() {
        return i32::try_from(n).ok();
    }
    let f = v.as_f64()?;
    if f.fract() != 0.0 || f < f64::from(i32::MIN) || f > f64::from(i32::MAX) {
        return None;
    }
    #[allow(clippy::cast_possible_truncation)]
    Some(f as i32)
}

/// Which ladder rung supplied each member of the temperature-coupled set.
///
/// Purely an intermediate inside [`InferenceConfig::resolve_layers_with_sources`]:
/// the two arms of the coupling rule each produce one of these, and the
/// provenance record is built from it. Named fields rather than a tuple
/// because both arms and the provenance construction read it positionally
/// otherwise, and the three are easy to transpose.
#[derive(Debug, Clone, Copy)]
struct CoupledLayers {
    repeat_penalty: Option<usize>,
    presence_penalty: Option<usize>,
    min_p: Option<usize>,
}

impl InferenceConfig {
    /// Merge another config into this one, preferring values from `other`.
    ///
    /// For each field, if `other` has Some(value), use it; otherwise keep self's value.
    /// This is useful for applying fallback chains.
    ///
    /// # Example
    ///
    /// ```rust
    /// use gglib_core::domain::InferenceConfig;
    ///
    /// let mut request = InferenceConfig {
    ///     temperature: Some(0.8),
    ///     ..Default::default()
    /// };
    ///
    /// let model_defaults = InferenceConfig {
    ///     temperature: Some(0.5),
    ///     top_p: Some(0.9),
    ///     ..Default::default()
    /// };
    ///
    /// request.merge_with(&model_defaults);
    /// assert_eq!(request.temperature, Some(0.8)); // Request value wins
    /// assert_eq!(request.top_p, Some(0.9));      // Fallback to model default
    /// ```
    pub const fn merge_with(&mut self, other: &Self) {
        if self.temperature.is_none() {
            self.temperature = other.temperature;
        }
        if self.top_p.is_none() {
            self.top_p = other.top_p;
        }
        if self.top_k.is_none() {
            self.top_k = other.top_k;
        }
        if self.max_tokens.is_none() {
            self.max_tokens = other.max_tokens;
        }
        if self.repeat_penalty.is_none() {
            self.repeat_penalty = other.repeat_penalty;
        }
        if self.presence_penalty.is_none() {
            self.presence_penalty = other.presence_penalty;
        }
        if self.min_p.is_none() {
            self.min_p = other.min_p;
        }
        if self.dry_multiplier.is_none() {
            self.dry_multiplier = other.dry_multiplier;
        }
        if self.dry_base.is_none() {
            self.dry_base = other.dry_base;
        }
        if self.dry_allowed_length.is_none() {
            self.dry_allowed_length = other.dry_allowed_length;
        }
        if self.dry_penalty_last_n.is_none() {
            self.dry_penalty_last_n = other.dry_penalty_last_n;
        }
    }

    /// Resolve an ordered list of sampling layers (highest priority first)
    /// into a single fully-resolved config, then fill anything still unset
    /// from `floor`.
    ///
    /// This is the one fold every multi-layer resolution surface goes
    /// through: [`resolve_with_profile`] wraps it for the simple
    /// request/profile/model/global shape, and
    /// [`crate::request_pipeline::sampling`] builds its own **six**-layer
    /// (`cli`, `client`, `profile`, `model`, `global`, `model auto-detected`)
    /// array and calls it directly. There is exactly one place that decides
    /// what "wins" means.
    ///
    /// # Uncoupled parameters
    ///
    /// `top_p`, `top_k`, `max_tokens` and the four DRY parameters gap-fill
    /// independently: each takes the first `Some` value found scanning the
    /// layers top to bottom.
    ///
    /// # Coupled parameters
    ///
    /// `presence_penalty`, `repeat_penalty` and `min_p` are only meaningful
    /// relative to how sharp the sampling distribution is, so they travel with
    /// the `temperature` they were chosen for. [`reasoning_profile`] pairs
    /// `temperature 1.0` with `presence_penalty 1.5` deliberately; a sparse
    /// profile that sets `temperature 0.2` and leaves the penalty unset must
    /// not inherit that `1.5` — that would run a recipe no layer ever
    /// intended, a penalty tuned for a broad distribution applied to a
    /// near-greedy one.
    ///
    /// So: `temperature` resolves to the first layer that sets one. If some
    /// layer does, the coupled trio comes *only* from that same layer — never
    /// a layer beneath it — falling to `floor` for anything that layer itself
    /// left unset. If **no** layer sets a temperature at all, nothing has been
    /// tuned against anything, so the trio gap-fills normally, exactly like
    /// the uncoupled parameters.
    ///
    /// # Why DRY is *not* coupled
    ///
    /// It was, briefly, on the symmetry argument that a repetition penalty is
    /// a repetition penalty. Verification showed the symmetry is false and the
    /// cost is real. `presence_penalty` and `repeat_penalty` are flat logit
    /// offsets competing directly with temperature's sharpening; DRY's
    /// strength is governed by its own `dry_base` and `dry_allowed_length`,
    /// and it targets verbatim sequence repetition — a failure mode that is
    /// *worse* at low temperature, not milder.
    ///
    /// Coupling it meant a layer naming a DRY value but no temperature lost
    /// that value silently whenever any lower layer named one, which is the
    /// default state of every `reasoning`-tagged model. Since no shipped
    /// profile and not [`reasoning_profile`] itself pairs a temperature with
    /// DRY values, the coupling protected nothing and cost the most natural
    /// way to switch DRY on. See #745.
    ///
    /// [`resolve_with_profile`]: Self::resolve_with_profile
    /// [`reasoning_profile`]: Self::reasoning_profile
    #[must_use]
    pub fn resolve_layers(layers: &[Option<&Self>], floor: &Self) -> Self {
        Self::resolve_layers_with_sources(layers, floor).0
    }

    /// Write the temperature-coupled set into `result` and report which rung
    /// supplied each member.
    ///
    /// Split out of [`resolve_layers_with_sources`] only for length; the rule
    /// it implements is documented on [`resolve_layers`]. `temperature` is the
    /// rung that claimed the temperature, if any — the whole coupling rule
    /// hangs off whether that is `Some`.
    ///
    /// [`resolve_layers_with_sources`]: Self::resolve_layers_with_sources
    /// [`resolve_layers`]: Self::resolve_layers
    fn resolve_coupled(
        layers: &[Option<&Self>],
        temperature: Option<usize>,
        result: &mut Self,
    ) -> CoupledLayers {
        let first = |declares: &dyn Fn(&Self) -> bool| -> Option<usize> {
            layers.iter().position(|l| l.is_some_and(declares))
        };

        // The layer claiming `temperature` supplies the whole set, including
        // the fields it left unset — those drop to the floor rather than
        // inheriting a value tuned for a temperature nobody chose.
        if let Some(claim) = temperature {
            let c = layers[claim].expect("index came from a Some layer");
            result.repeat_penalty = c.repeat_penalty;
            result.presence_penalty = c.presence_penalty;
            result.min_p = c.min_p;
            return CoupledLayers {
                repeat_penalty: c.repeat_penalty.and(Some(claim)),
                presence_penalty: c.presence_penalty.and(Some(claim)),
                min_p: c.min_p.and(Some(claim)),
            };
        }

        // Nothing was tuned against anything, so the set gap-fills like any
        // uncoupled parameter.
        let found = CoupledLayers {
            repeat_penalty: first(&|c| c.repeat_penalty.is_some()),
            presence_penalty: first(&|c| c.presence_penalty.is_some()),
            min_p: first(&|c| c.min_p.is_some()),
        };
        result.repeat_penalty = found
            .repeat_penalty
            .and_then(|i| layers[i].and_then(|c| c.repeat_penalty));
        result.presence_penalty = found
            .presence_penalty
            .and_then(|i| layers[i].and_then(|c| c.presence_penalty));
        result.min_p = found.min_p.and_then(|i| layers[i].and_then(|c| c.min_p));
        found
    }

    /// [`resolve_layers`] plus a record of which layer supplied each field.
    ///
    /// This is the implementation; [`resolve_layers`] delegates here and
    /// discards the provenance. Values and provenance therefore come from one
    /// pass over one ladder and cannot disagree — a second function that
    /// re-derived the rules would eventually explain a decision the resolution
    /// did not take, which is exactly what the `describe_provenance` helper
    /// this replaced had already started doing.
    ///
    /// See [`FieldSources`] for how to read the result, and [`resolve_layers`]
    /// for the coupling rule the sources reflect.
    ///
    /// [`resolve_layers`]: Self::resolve_layers
    #[must_use]
    pub fn resolve_layers_with_sources(
        layers: &[Option<&Self>],
        floor: &Self,
    ) -> (Self, FieldSources) {
        // Index into `layers` — not into the flattened iterator — so a caller
        // can map it back to the name it gave that rung.
        let first = |declares: &dyn Fn(&Self) -> bool| -> Option<usize> {
            layers.iter().position(|l| l.is_some_and(declares))
        };

        let mut result = Self::default();

        // Uncoupled: each takes the first layer that names it, independently.
        let top_p = first(&|c| c.top_p.is_some());
        let top_k = first(&|c| c.top_k.is_some());
        let max_tokens = first(&|c| c.max_tokens.is_some());
        let temperature = first(&|c| c.temperature.is_some());
        let dry_multiplier = first(&|c| c.dry_multiplier.is_some());
        let dry_base = first(&|c| c.dry_base.is_some());
        let dry_allowed_length = first(&|c| c.dry_allowed_length.is_some());
        let dry_penalty_last_n = first(&|c| c.dry_penalty_last_n.is_some());

        result.top_p = top_p.and_then(|i| layers[i].and_then(|c| c.top_p));
        result.top_k = top_k.and_then(|i| layers[i].and_then(|c| c.top_k));
        result.max_tokens = max_tokens.and_then(|i| layers[i].and_then(|c| c.max_tokens));
        result.temperature = temperature.and_then(|i| layers[i].and_then(|c| c.temperature));
        result.dry_multiplier =
            dry_multiplier.and_then(|i| layers[i].and_then(|c| c.dry_multiplier));
        result.dry_base = dry_base.and_then(|i| layers[i].and_then(|c| c.dry_base));
        result.dry_allowed_length =
            dry_allowed_length.and_then(|i| layers[i].and_then(|c| c.dry_allowed_length));
        result.dry_penalty_last_n =
            dry_penalty_last_n.and_then(|i| layers[i].and_then(|c| c.dry_penalty_last_n));

        let coupled_layers = Self::resolve_coupled(layers, temperature, &mut result);

        result.merge_with(floor);

        // A field no layer claimed came from the floor — or from nowhere, when
        // the floor has none either, which is whatever `with_hardcoded_defaults`
        // leaves unset rather than a list worth restating here.
        //
        // The coupling rule is checked **before** the floor's emptiness, and
        // the order is load-bearing. `Unset` means "nobody named this"; when a
        // layer named it and the coupling rule passed it over, that is a
        // different and more interesting fact, and it stays true whether or
        // not the floor then had a value to offer. Testing `!has_floor` first
        // was harmless while the floor filled all seven, and became a silent
        // loss of provenance the moment ADR 0003 emptied six of them — the
        // coupled trio would have reported as a plain absence.
        let coupled = temperature.is_some();
        let source = |won: Option<usize>, has_floor: bool, is_coupled: bool| match won {
            Some(i) => ParamSource::Layer(i),
            None if is_coupled => ParamSource::FloorCoupled,
            None if !has_floor => ParamSource::Unset,
            None => ParamSource::Floor,
        };

        let sources = FieldSources {
            temperature: source(temperature, floor.temperature.is_some(), false),
            top_p: source(top_p, floor.top_p.is_some(), false),
            top_k: source(top_k, floor.top_k.is_some(), false),
            presence_penalty: source(
                coupled_layers.presence_penalty,
                floor.presence_penalty.is_some(),
                coupled,
            ),
            repeat_penalty: source(
                coupled_layers.repeat_penalty,
                floor.repeat_penalty.is_some(),
                coupled,
            ),
            min_p: source(coupled_layers.min_p, floor.min_p.is_some(), coupled),
            dry_multiplier: source(dry_multiplier, floor.dry_multiplier.is_some(), false),
            dry_base: source(dry_base, floor.dry_base.is_some(), false),
            dry_allowed_length: source(
                dry_allowed_length,
                floor.dry_allowed_length.is_some(),
                false,
            ),
            dry_penalty_last_n: source(
                dry_penalty_last_n,
                floor.dry_penalty_last_n.is_some(),
                false,
            ),
            max_tokens: source(max_tokens, floor.max_tokens.is_some(), false),
        };

        (result, sources)
    }

    /// The floor beneath every sampling ladder: what gglib asserts when no
    /// layer named a value.
    ///
    /// # It asserts one parameter, not seven
    ///
    /// [ADR 0003] measured this floor against a bare `llama-server` on the
    /// pinned build and found six of its seven values were *exactly* the
    /// upstream default:
    ///
    /// ```text
    ///   parameter          gglib floor   upstream   verdict
    ///   temperature                0.7        0.8   DIVERGES -> policy
    ///   top_p                     0.95       0.95   EQUALS   -> deleted
    ///   top_k                       40         40   EQUALS   -> deleted
    ///   repeat_penalty             1.0        1.0   EQUALS   -> deleted
    ///   presence_penalty           0.0        0.0   EQUALS   -> deleted
    ///   min_p                     0.05       0.05   EQUALS   -> deleted
    ///   dry_multiplier             0.0        0.0   EQUALS   -> deleted
    /// ```
    ///
    /// Restating a value that is already the answer is not a decision, it is
    /// a redundant assertion — and a costly one, because it silently overrides
    /// whatever upstream chooses next. #739 was exactly that failure: a floor
    /// of `min_p: 0.0` disabled the tail cut on every untuned request, and
    /// nothing in the system was positioned to notice. Six such overrides are
    /// now impossible.
    ///
    /// The six are **deferred**, not disabled. Nothing is emitted for them, so
    /// llama.cpp applies its own default — which on this build is the same
    /// number that used to be written here. Provenance reports them as
    /// [`ParamSource::Unset`](crate::domain::ParamSource::Unset), which is
    /// precisely what deferral is: gglib names no value.
    ///
    /// # `temperature: 0.7` stays, and upstream's is 0.8
    ///
    /// The one genuine policy choice in the set, and stated here because an
    /// undocumented divergence is how the other six became invisible in the
    /// first place. gglib decodes slightly more conservatively than
    /// llama.cpp's default for agentic work.
    ///
    /// # The floor is no longer uniform
    ///
    /// [`reasoning_floor`] still asserts `presence_penalty: 1.0` and
    /// `min_p: 0.0` for `reasoning`-tagged models, which are class-aware
    /// policy llama.cpp has no notion of. So after this change `min_p` is
    /// asserted for reasoning models and deferred for every other model.
    /// That is the correct shape and it needs saying out loud, because it is
    /// the first time the floor has differed by model class in what it
    /// *names* rather than only in what it names it as.
    ///
    /// # Deferral is safe only while the build is pinned
    ///
    /// [ADR 0002] pins the llama.cpp build; [ADR 0004]'s `/props` baseline
    /// check reads the default table back and flags any field that moves.
    /// That pairing is what makes deleting a value behaviour-preserving
    /// rather than hopeful.
    ///
    /// [ADR 0002]: https://github.com/mmogr/gglib/blob/main/docs/adr/0002-defer-tool-call-constraint-to-llama-cpp.md
    /// [ADR 0003]: https://github.com/mmogr/gglib/blob/main/docs/adr/0003-defer-sampler-defaults-to-llama-cpp.md
    /// [ADR 0004]: https://github.com/mmogr/gglib/blob/main/docs/adr/0004-observe-the-sampling-boundary.md
    /// [`reasoning_floor`]: Self::reasoning_floor
    ///
    /// # `max_tokens` has no fallback
    ///
    /// It is deliberately `None`. Resolution force-writes every `Some` field
    /// into the outgoing request, so a value here would cap *every* request
    /// that did not name its own — silently truncating long answers. Left
    /// unset, no `max_tokens` key is emitted and llama-server applies its own
    /// `n_predict` default of `-1`, generating until a stop token or the
    /// context limit.
    ///
    /// Omitting the key is exactly equivalent to sending `-1` (llama.cpp's
    /// `has_budget()` treats `-1` as limitless) and is the better of the two:
    /// `max_tokens: -1` is invalid under the `OpenAI` schema, which requires a
    /// positive integer, so a strict client or intermediary proxy may reject
    /// it. Omission keeps the forwarded body `OpenAI`-legal.
    ///
    /// Explicit per-request, per-profile, and per-model values are unaffected —
    /// [`reasoning_profile`] still sets its own ceiling.
    ///
    /// # A note on `min_p`, because it moved twice
    ///
    /// #739 changed it from `0.0` to `0.05`, correctly: `0.0` reads like an
    /// absence but was not one — [`to_openai_json_patch`] drops only `None`,
    /// so the floor was *explicitly disabling* the tail cut on every untuned
    /// request. The fix was right, and the mechanism it used was the problem.
    /// #739 restated upstream's value to keep it "visible as `min_p=floor` in
    /// sampling provenance instead of reporting as unset", which bought
    /// visibility at the price of a permanent silent override. Deferral is the
    /// better answer to the same objection: it reports as unset *because it is
    /// unset*, and [ADR 0004]'s readback names llama.cpp's own number instead
    /// of gglib restating it.
    ///
    /// [`reasoning_profile`]: Self::reasoning_profile
    /// [`to_openai_json_patch`]: Self::to_openai_json_patch
    #[must_use]
    pub const fn with_hardcoded_defaults() -> Self {
        Self {
            // The one value gglib asserts. Upstream's is 0.8; see above.
            temperature: Some(0.7),
            // Everything below is deferred to llama.cpp, which is a decision
            // and not an omission — ADR 0003 finding 1 measured each of them
            // equal to the upstream default on the pinned build. Setting any
            // of these again means overriding whatever upstream chooses next,
            // so do it only with a measurement saying upstream is wrong.
            top_p: None,
            top_k: None,
            repeat_penalty: None,
            presence_penalty: None,
            min_p: None,
            // DRY stays off, and now says so by silence rather than by
            // asserting the zero llama.cpp already defaults to. Enabling it
            // fleet-wide is a tuning decision for a per-model or per-profile
            // layer with sweep data behind it, not for the floor every untuned
            // model lands on.
            dry_multiplier: None,
            // Never had a floor: with the multiplier off they have no effect,
            // and asserting values would claim a recipe nobody has measured.
            dry_base: None,
            dry_allowed_length: None,
            dry_penalty_last_n: None,
            // No fallback by design — see above.
            max_tokens: None,
        }
    }

    /// The coupled-trio floor for models tagged `reasoning`.
    ///
    /// [`resolve_layers`] falls back to a floor once it has decided which
    /// layer (if any) claims the coupled set and that layer left a field
    /// unset. [`with_hardcoded_defaults`]'s neutral `presence_penalty: 0.0` is
    /// the right floor for most models, but wrong for a `reasoning`-tagged
    /// one: those degrade under greedy or near-greedy decoding into
    /// repetitive reasoning loops (see [`reasoning_profile`], which pairs
    /// `presence_penalty: 1.5` with `temperature: 1.0` specifically to
    /// prevent this). `1.0` keeps a real guard in place at the floor without
    /// asserting the full recipe tuned for a different temperature.
    ///
    /// `min_p` is pinned to `0.0` for the same class-specific reason: Qwen3.6's
    /// published guidance is to disable min-p on these models, which
    /// [`reasoning_profile`] already encodes.
    ///
    /// # These two are now the only class-specific *assertions*
    ///
    /// The neutral floor used to name `min_p: 0.05` and `presence_penalty:
    /// 0.0`, so this function read as "the same seven values, two of them
    /// different". [ADR 0003] deferred both of those to llama.cpp, so it now
    /// reads as "two values the neutral floor does not name at all".
    ///
    /// The consequence is worth stating because it makes the floor non-uniform
    /// in a way it never was: **`min_p` is asserted for reasoning models and
    /// deferred for everything else.** A reasoning model gets `min_p: 0.0` on
    /// the wire; every other model gets no `min_p` key and llama.cpp's own
    /// 0.05. That asymmetry is deliberate — one is a measured divergence from
    /// upstream, the other is agreement with it — but it will look like a bug
    /// to anyone diffing two requests without this paragraph.
    ///
    /// `presence_penalty: 1.0` is the same shape: asserted here, deferred
    /// elsewhere.
    ///
    /// [ADR 0003]: https://github.com/mmogr/gglib/blob/main/docs/adr/0003-defer-sampler-defaults-to-llama-cpp.md
    ///
    /// [`resolve_layers`]: Self::resolve_layers
    /// [`with_hardcoded_defaults`]: Self::with_hardcoded_defaults
    /// [`reasoning_profile`]: Self::reasoning_profile
    #[must_use]
    pub const fn reasoning_floor() -> Self {
        Self {
            presence_penalty: Some(1.0),
            min_p: Some(0.0),
            ..Self::with_hardcoded_defaults()
        }
    }

    /// The highest temperature an agentic turn should decode at.
    ///
    /// A turn that carries tools may emit structured output, where creativity
    /// is only ever a defect. This is the ceiling that caps it — applied by
    /// [`crate::request_pipeline::sampling`] *after* resolution, and only over
    /// a value nobody deliberately chose. It never raises a temperature.
    ///
    /// # Why reasoning models get a much higher ceiling
    ///
    /// A `reasoning` model does not decode its tool call in isolation: the
    /// `<think>` block and the call are one completion under one sampler
    /// configuration, so any ceiling imposed for the sake of structured output
    /// lands on the reasoning phase too. Both vendors warn about exactly this
    /// — Qwen3 specifies ~0.6 for thinking mode and says not to use greedy
    /// decoding, and DeepSeek-R1 specifies 0.5–0.7 for the same reason. Below
    /// that range these models degrade into endless repetition, which the
    /// proxy's own loop guard would then reject as a 400. Capping a thinking
    /// model near-greedy manufactures the failure that guard exists to catch.
    ///
    /// `0.3` for everything else is low enough to steady structured output
    /// without being greedy.
    ///
    /// # Why a ceiling and not a floor
    ///
    /// The floor this replaced could never fire on the models that most needed
    /// it. A `reasoning`-tagged model carries an auto-detected recipe naming
    /// `temperature: 1.0`, and any layer outranks a floor — so the adjustment
    /// was inert on precisely the models used for agentic coding. A ceiling
    /// gated on provenance fires there and stays out of the way everywhere a
    /// person actually made a choice.
    #[must_use]
    pub const fn agentic_temperature_ceiling(is_reasoning: bool) -> f32 {
        if is_reasoning { 0.6 } else { 0.3 }
    }

    /// Return a recommended [`InferenceConfig`] profile for reasoning / thinking models.
    ///
    /// Applied automatically at import time when the `"reasoning"` capability tag is
    /// detected (e.g. Qwen3.6, `DeepSeek-R1`, `QwQ`). Values follow the Qwen3.6 upstream
    /// guidance for **thinking mode — general tasks** and are conservative enough to
    /// work well across all thinking-capable models.
    ///
    /// | Parameter | Value | Rationale |
    /// |-----------|-------|-----------|
    /// | `temperature` | 1.0 | Recommended thinking-mode baseline |
    /// | `top_p` | 0.95 | Broad nucleus; standard for reasoning |
    /// | `top_k` | 20 | Tighter than the 40 fallback; suppresses low-quality tokens |
    /// | `max_tokens` | 8192 | Safe out-of-the-box ceiling; increase for complex tasks |
    /// | `repeat_penalty` | 1.0 | No penalty; `presence_penalty` handles anti-repetition |
    /// | `presence_penalty` | 1.5 | Prevents repetitive reasoning loops |
    /// | `min_p` | 0.0 | Explicitly disabled per Qwen3.6 spec |
    ///
    /// Users can override any parameter with `gglib model update <id> --<flag>` or
    /// the equivalent UI control.
    #[must_use]
    pub const fn reasoning_profile() -> Self {
        Self {
            temperature: Some(1.0),
            top_p: Some(0.95),
            top_k: Some(20),
            max_tokens: Some(8192),
            repeat_penalty: Some(1.0),
            presence_penalty: Some(1.5),
            min_p: Some(0.0),
            // Deliberately unset, and this must stay that way: legacy rows are
            // classified as auto-detected by comparing their stored defaults
            // against this recipe verbatim (`resolve_defaults_origin`). Rows
            // written before DRY existed deserialize these as `None`, so any
            // value here would make every one of them compare unequal and
            // silently reclassify as user-set, moving them up a resolution
            // rung.
            dry_multiplier: None,
            dry_base: None,
            dry_allowed_length: None,
            dry_penalty_last_n: None,
        }
    }

    /// Resolve inference parameters using the 4-level hierarchy.
    ///
    /// Equivalent to [`resolve_with_profile`] with no profile selected — see
    /// there for the merge order. This is the entry point for surfaces that
    /// have no notion of a named profile (`gglib serve`, `gglib chat`,
    /// `gglib q`, the Web UI chat API).
    ///
    /// `model_ctx` carries the two facts about the target model that change
    /// how resolution behaves — see [`ModelSamplingContext`],
    /// [`resolve_layers`] and [`reasoning_floor`].
    ///
    /// # Example
    ///
    /// ```rust
    /// use gglib_core::domain::{InferenceConfig, ModelSamplingContext};
    ///
    /// let request = InferenceConfig { temperature: Some(0.9), ..Default::default() };
    /// let model   = InferenceConfig { temperature: Some(0.5), top_p: Some(0.8), ..Default::default() };
    ///
    /// let resolved = request.resolve_with_defaults(Some(&model), None, ModelSamplingContext::default());
    /// assert_eq!(resolved.temperature, Some(0.9)); // request wins
    /// assert_eq!(resolved.top_p,       Some(0.8)); // model fills in
    /// assert_eq!(resolved.top_k,       None);      // no layer named it, and
    ///                                              // the floor defers top_k
    ///                                              // to llama.cpp (ADR 0003)
    /// ```
    ///
    /// [`resolve_with_profile`]: Self::resolve_with_profile
    /// [`resolve_layers`]: Self::resolve_layers
    /// [`reasoning_floor`]: Self::reasoning_floor
    #[must_use]
    pub fn resolve_with_defaults(
        self,
        model: Option<&Self>,
        global: Option<&Self>,
        model_ctx: ModelSamplingContext,
    ) -> Self {
        self.resolve_with_profile(None, model, global, model_ctx)
    }

    /// Resolve inference parameters using the full 5-level hierarchy.
    ///
    /// Applies fallback layers in order, with each layer filling only `None`
    /// fields from `self` — explicit values are never overwritten:
    ///
    /// 1. `self` — caller-supplied overrides (request params, CLI flags, etc.)
    /// 2. `profile` — the named profile the request selected, if any
    /// 3. `model` — per-model stored defaults, *if user-set*
    /// 4. `global` — global settings defaults
    /// 5. `model` again, *if auto-detected* — see below
    /// 6. the model-class floor — [`reasoning_floor`] when
    ///    `model_ctx.is_reasoning`, otherwise [`with_hardcoded_defaults`]
    ///
    /// This is the single source of truth for inference parameter resolution
    /// across every gglib surface that does not need its own layer set;
    /// [`resolve_with_defaults`] delegates here so there is exactly one merge
    /// order to reason about and to test.
    /// [`crate::request_pipeline::sampling`] needs a sixth rung (the client's
    /// own request, sitting *below* the CLI override rather than between
    /// `self` and `profile`) and calls the underlying [`resolve_layers`]
    /// directly for that reason — the merge semantics are identical either
    /// way.
    ///
    /// # Why the profile sits above the model
    ///
    /// Selecting `model:coding` is an explicit act by the caller, so it has to
    /// beat the model's stored defaults or it would appear to do nothing on any
    /// model that has them. Because profiles are *sparse* (see
    /// [`crate::domain::inference_profile`]), outranking the model layer costs
    /// nothing for parameters the profile does not set — those still resolve
    /// from the model, which is what keeps one global profile safe to apply
    /// across differing architectures.
    ///
    /// # Why `model` can rank below `global`
    ///
    /// `model` is only ever a stand-in for `Model.inference_defaults`, which
    /// gets written two different ways (see [`DefaultsOrigin`]): a person
    /// tuning it deliberately, or gglib's own import-time guess for any
    /// model tagged `reasoning`. Those deserve different authority. A
    /// deliberate per-model choice should keep outranking the operator's
    /// global defaults — that is what "per-model" means. A guess nobody
    /// reviewed should not: it silently shadowed the user's own configured
    /// global settings, which is how #685 happened. `model_ctx.defaults_origin`
    /// decides which rung `model` occupies for this call — never both at
    /// once, since only one of rungs 3 and 5 is ever populated for a given
    /// model.
    ///
    /// # Temperature-tuned parameters do not fall through
    ///
    /// See [`resolve_layers`] for the full rule. In short: once a layer
    /// declares a `temperature`, lower layers may not contribute
    /// `presence_penalty`, `repeat_penalty` or `min_p` — those resolve from
    /// the claiming layer alone, falling to the class floor if it left them
    /// unset.
    ///
    /// # Example
    ///
    /// ```rust
    /// use gglib_core::domain::{InferenceConfig, ModelSamplingContext};
    ///
    /// // A sparse profile: sets temperature, says nothing about anything else.
    /// let profile = InferenceConfig { temperature: Some(0.2), ..Default::default() };
    /// // A thinking model's stored defaults: 1.5 is tuned for temperature 1.0.
    /// let model = InferenceConfig {
    ///     temperature: Some(1.0),
    ///     presence_penalty: Some(1.5),
    ///     top_k: Some(20),
    ///     ..Default::default()
    /// };
    /// let model_ctx = ModelSamplingContext { is_reasoning: true, ..Default::default() };
    ///
    /// let resolved = InferenceConfig::default()
    ///     .resolve_with_profile(Some(&profile), Some(&model), None, model_ctx);
    ///
    /// assert_eq!(resolved.temperature,      Some(0.2)); // profile beats model
    /// assert_eq!(resolved.presence_penalty, Some(1.0)); // reasoning floor, NOT the model's 1.5
    /// assert_eq!(resolved.top_k,            Some(20));  // untuned: still fills
    /// ```
    ///
    /// [`resolve_layers`]: Self::resolve_layers
    /// [`reasoning_floor`]: Self::reasoning_floor
    /// [`with_hardcoded_defaults`]: Self::with_hardcoded_defaults
    /// [`resolve_with_defaults`]: Self::resolve_with_defaults
    #[must_use]
    pub fn resolve_with_profile(
        self,
        profile: Option<&Self>,
        model: Option<&Self>,
        global: Option<&Self>,
        model_ctx: ModelSamplingContext,
    ) -> Self {
        self.resolve_with_profile_explained(profile, model, global, model_ctx)
            .0
    }

    /// [`resolve_with_profile`] plus a record of which rung supplied each
    /// field.
    ///
    /// This is the implementation; [`resolve_with_profile`] delegates here and
    /// discards the provenance, so the ladder — including the user/auto rung
    /// split and the floor selection — is built exactly once.
    ///
    /// Map a [`ParamSource::Layer`] index back to a rung with
    /// [`SamplingLayer::from_index`], which is kept beside this ladder for
    /// that purpose.
    ///
    /// [`resolve_with_profile`]: Self::resolve_with_profile
    /// [`SamplingLayer::from_index`]: crate::domain::SamplingLayer::from_index
    /// [`ParamSource::Layer`]: crate::domain::ParamSource::Layer
    #[must_use]
    pub fn resolve_with_profile_explained(
        self,
        profile: Option<&Self>,
        model: Option<&Self>,
        global: Option<&Self>,
        model_ctx: ModelSamplingContext,
    ) -> (Self, FieldSources) {
        let floor = if model_ctx.is_reasoning {
            Self::reasoning_floor()
        } else {
            Self::with_hardcoded_defaults()
        };
        let (user_model, auto_model) = match model_ctx.defaults_origin {
            Some(DefaultsOrigin::AutoDetected) => (None, model),
            _ => (model, None),
        };
        Self::resolve_layers_with_sources(
            &[Some(&self), profile, user_model, global, auto_model],
            &floor,
        )
    }

    /// Parse inference parameters from an OpenAI-format JSON body (`snake_case` keys).
    ///
    /// Missing keys, explicit `null`s and keys this type does not model all
    /// yield `None` for that field and leave the rest untouched.
    ///
    /// This is the inverse of [`to_openai_json_patch`]. Use
    /// [`extract_client_sampling`] when the caller can report what went wrong.
    ///
    /// [`to_openai_json_patch`]: Self::to_openai_json_patch
    /// [`extract_client_sampling`]: Self::extract_client_sampling
    #[must_use]
    pub fn from_openai_json(value: &serde_json::Value) -> Self {
        Self::extract_client_sampling(value).0
    }

    /// [`from_openai_json`] plus what it had to reject or normalise.
    ///
    /// # One bad field must not cost the other ten
    ///
    /// This read used to camel-case the whole body and hand it to
    /// `serde_json::from_value(..).unwrap_or_default()`. Serde parses an
    /// object as a unit, so a single wrongly-typed key failed the whole
    /// deserialise and `unwrap_or_default()` returned an all-`None` config —
    /// silently discarding every sampling value the client sent, with no log
    /// and no test covering the failure path.
    ///
    /// Reading field by field means a bad `max_tokens` costs `max_tokens` and
    /// nothing else.
    ///
    /// # The coercion policy is upstream's, not ours
    ///
    /// [ADR 0003] finding 6 measured what llama.cpp itself accepts on the
    /// pinned build, so this does not have to invent a policy:
    ///
    /// | sent | llama.cpp | here |
    /// |---|---|---|
    /// | `max_tokens: -1` | 200 | [`Normalised`] to `None` — omission already means "no limit" |
    /// | `top_k: 40.0` | 200 | accepted as `40`; a *fractional* float is rejected |
    /// | `temperature: "0.7"` | 400 | [`Rejected`] — a numeric string is a client bug, and quietly parsing it teaches nobody |
    ///
    /// The principle is to accept what upstream accepts and reject what
    /// upstream rejects, so gglib never becomes the stricter of the two on a
    /// value that would have worked. Before this change it was: llama.cpp
    /// takes `max_tokens: -1` and gglib threw away the entire layer over it.
    ///
    /// [ADR 0003]: https://github.com/mmogr/gglib/blob/main/docs/adr/0003-defer-sampler-defaults-to-llama-cpp.md
    /// [`Normalised`]: FieldIssue::Normalised
    /// [`Rejected`]: FieldIssue::Rejected
    #[must_use]
    pub fn extract_client_sampling(value: &serde_json::Value) -> (Self, Vec<FieldIssue>) {
        let Some(obj) = value.as_object() else {
            return (Self::default(), Vec::new());
        };
        let mut issues = Vec::new();

        let cfg = Self {
            temperature: read_f32(obj, "temperature", &mut issues),
            top_p: read_f32(obj, "top_p", &mut issues),
            top_k: read_i32(obj, "top_k", &mut issues),
            max_tokens: read_max_tokens(obj, &mut issues),
            repeat_penalty: read_f32(obj, "repeat_penalty", &mut issues),
            presence_penalty: read_f32(obj, "presence_penalty", &mut issues),
            min_p: read_f32(obj, "min_p", &mut issues),
            dry_multiplier: read_f32(obj, "dry_multiplier", &mut issues),
            dry_base: read_f32(obj, "dry_base", &mut issues),
            dry_allowed_length: read_i32(obj, "dry_allowed_length", &mut issues),
            dry_penalty_last_n: read_i32(obj, "dry_penalty_last_n", &mut issues),
        };

        (cfg, issues)
    }

    /// Serialise as an OpenAI-format JSON patch (`snake_case` keys, `Some` fields only).
    ///
    /// Uses `serde` to produce the camelCase form, then renames each key to
    /// `snake_case` via [`camel_to_snake`]. Only `Some` fields are emitted — `None`
    /// values are filtered out. The returned map can be merged directly into an
    /// OpenAI-compatible request body with `body_obj.insert(k, v)`.
    ///
    /// This is the inverse of [`from_openai_json`].
    ///
    /// [`from_openai_json`]: Self::from_openai_json
    #[must_use]
    pub fn to_openai_json_patch(&self) -> serde_json::Map<String, serde_json::Value> {
        let camel = serde_json::to_value(self).unwrap_or_default();
        camel
            .as_object()
            .into_iter()
            .flatten()
            .filter(|(_, v)| !v.is_null())
            .map(|(k, v)| (camel_to_snake(k), v.clone()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_is_all_none() {
        let config = InferenceConfig::default();
        assert!(config.temperature.is_none());
        assert!(config.top_p.is_none());
        assert!(config.top_k.is_none());
        assert!(config.max_tokens.is_none());
        assert!(config.repeat_penalty.is_none());
        assert!(config.presence_penalty.is_none());
        assert!(config.min_p.is_none());
    }

    #[test]
    fn test_merge_with_prefers_self() {
        let mut request = InferenceConfig {
            temperature: Some(0.8),
            top_p: None,
            ..Default::default()
        };

        let model_defaults = InferenceConfig {
            temperature: Some(0.5),
            top_p: Some(0.9),
            top_k: Some(50),
            ..Default::default()
        };

        request.merge_with(&model_defaults);

        assert_eq!(request.temperature, Some(0.8)); // Request wins
        assert_eq!(request.top_p, Some(0.9)); // Fallback to model
        assert_eq!(request.top_k, Some(50)); // Fallback to model
        assert!(request.max_tokens.is_none()); // Still None
    }

    /// The reasoning floor differs from the hardcoded floor in exactly two
    /// fields, both class-specific: a real anti-repetition guard where the
    /// neutral floor has none, and min-p disabled per Qwen3.6's guidance
    /// where the neutral floor matches llama.cpp.
    #[test]
    fn test_reasoning_floor_differs_only_in_presence_penalty_and_min_p() {
        let neutral = InferenceConfig::with_hardcoded_defaults();
        let reasoning = InferenceConfig::reasoning_floor();

        assert_eq!(reasoning.presence_penalty, Some(1.0));
        assert_ne!(reasoning.presence_penalty, neutral.presence_penalty);

        assert_eq!(reasoning.min_p, Some(0.0));
        assert_ne!(reasoning.min_p, neutral.min_p);

        assert_eq!(reasoning.temperature, neutral.temperature);
        assert_eq!(reasoning.top_p, neutral.top_p);
        assert_eq!(reasoning.top_k, neutral.top_k);
        assert_eq!(reasoning.max_tokens, neutral.max_tokens);
        assert_eq!(reasoning.repeat_penalty, neutral.repeat_penalty);
    }

    /// If nothing in the stack ever declares a temperature, nothing has been
    /// "tuned" against anything — the coupled set must gap-fill exactly like
    /// any other parameter, from whichever layer sets it first, rather than
    /// jump straight to the floor.
    #[test]
    fn test_coupled_trio_gap_fills_normally_when_no_layer_sets_temperature() {
        let profile = InferenceConfig {
            presence_penalty: Some(0.3),
            ..Default::default()
        };
        let model = InferenceConfig {
            presence_penalty: Some(0.5),
            repeat_penalty: Some(1.2),
            ..Default::default()
        };

        let resolved = InferenceConfig::default().resolve_with_profile(
            Some(&profile),
            Some(&model),
            None,
            ModelSamplingContext::default(),
        );

        assert_eq!(resolved.temperature, Some(0.7), "hardcoded fallback");
        assert_eq!(
            resolved.presence_penalty,
            Some(0.3),
            "profile's own value, not suppressed just because no layer set a temperature"
        );
        assert_eq!(
            resolved.repeat_penalty,
            Some(1.2),
            "model fills in what the profile left unset"
        );
    }

    /// An unset `max_tokens` must not be written into the request body.
    ///
    /// This used to check a second route as well — a `-n` flag on the launch
    /// command line, which is the more dangerous of the two because it sets
    /// `global_params.n_predict`, a server-wide ceiling overriding even a
    /// per-request `-1`. ADR 0003 deleted `to_cli_args`, so that route no
    /// longer exists for any parameter and there is nothing left to assert
    /// about it: the guarantee moved from a test to the type system.
    #[test]
    fn test_unset_max_tokens_is_not_written_into_the_body() {
        let resolved = InferenceConfig::default().resolve_with_defaults(
            None,
            None,
            ModelSamplingContext::default(),
        );

        assert!(
            !resolved.to_openai_json_patch().contains_key("max_tokens"),
            "an unset max_tokens must not be written into the request body"
        );
    }

    /// This change removed the *fallback*, not the parameter.
    #[test]
    fn test_explicit_max_tokens_is_still_forwarded() {
        let resolved = InferenceConfig {
            max_tokens: Some(512),
            ..Default::default()
        }
        .resolve_with_defaults(None, None, ModelSamplingContext::default());

        assert_eq!(resolved.max_tokens, Some(512));
        assert_eq!(
            resolved.to_openai_json_patch().get("max_tokens"),
            Some(&serde_json::json!(512))
        );
    }

    /// The floor asserts exactly one parameter, and it is the one ADR 0003
    /// measured as diverging from upstream.
    ///
    /// Written as an exhaustive field-by-field check rather than an equality
    /// against a literal so that adding a value back is a *failure with a
    /// name*, not a diff someone re-blesses. Every `None` here is a field
    /// llama.cpp supplies; setting one again overrides whatever upstream
    /// chooses next, which is #739's failure mode.
    #[test]
    fn the_floor_asserts_only_the_value_that_diverges_from_upstream() {
        let floor = InferenceConfig::with_hardcoded_defaults();

        assert_eq!(
            floor.temperature,
            Some(0.7),
            "the one genuine policy choice; upstream's is 0.8"
        );

        for (field, value) in [
            ("top_p", floor.top_p),
            ("min_p", floor.min_p),
            ("repeat_penalty", floor.repeat_penalty),
            ("presence_penalty", floor.presence_penalty),
            ("dry_multiplier", floor.dry_multiplier),
            ("dry_base", floor.dry_base),
        ] {
            assert_eq!(value, None, "{field} is deferred to llama.cpp (ADR 0003)");
        }
        assert_eq!(floor.top_k, None, "top_k is deferred to llama.cpp");
        assert_eq!(
            floor.max_tokens, None,
            "max_tokens has no fallback by design"
        );
        assert_eq!(floor.dry_allowed_length, None);
        assert_eq!(floor.dry_penalty_last_n, None);
    }

    /// The non-uniformity ADR 0003 decision 3 called out: `min_p` is asserted
    /// for reasoning models and deferred for everything else. Pinned because
    /// it reads like a bug when you diff two requests.
    #[test]
    fn min_p_is_asserted_for_reasoning_models_and_deferred_for_the_rest() {
        assert_eq!(InferenceConfig::reasoning_floor().min_p, Some(0.0));
        assert_eq!(InferenceConfig::with_hardcoded_defaults().min_p, None);

        assert_eq!(
            InferenceConfig::reasoning_floor().presence_penalty,
            Some(1.0)
        );
        assert_eq!(
            InferenceConfig::with_hardcoded_defaults().presence_penalty,
            None
        );
    }

    /// The whole point of the deferral: an untuned request names one sampler,
    /// not seven, so llama.cpp's own defaults apply to the rest.
    #[test]
    fn an_untuned_request_body_carries_only_the_temperature() {
        let resolved = InferenceConfig::default().resolve_with_defaults(
            None,
            None,
            ModelSamplingContext::default(),
        );
        let patch = resolved.to_openai_json_patch();

        assert_eq!(
            patch.keys().collect::<Vec<_>>(),
            vec!["temperature"],
            "anything else here is gglib overriding an upstream default: {patch:?}"
        );
    }

    #[test]
    fn test_reasoning_profile() {
        let profile = InferenceConfig::reasoning_profile();
        assert_eq!(profile.temperature, Some(1.0));
        assert_eq!(profile.top_p, Some(0.95));
        assert_eq!(profile.top_k, Some(20));
        assert_eq!(profile.max_tokens, Some(8192));
        assert_eq!(profile.repeat_penalty, Some(1.0));
        assert_eq!(profile.presence_penalty, Some(1.5));
        assert_eq!(profile.min_p, Some(0.0));
    }

    #[test]
    fn test_serialization() {
        let config = InferenceConfig {
            temperature: Some(0.7),
            top_p: Some(0.9),
            top_k: None,
            max_tokens: Some(1024),
            repeat_penalty: None,
            presence_penalty: None,
            min_p: None,
            // A set and an unset DRY field, so the round-trip covers both the
            // camelCase rename and the `Option` shape for the new parameters.
            dry_multiplier: Some(0.8),
            dry_base: None,
            dry_allowed_length: Some(2),
            dry_penalty_last_n: Some(-1),
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: InferenceConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(config, deserialized);
    }

    #[test]
    fn test_camel_to_snake() {
        assert_eq!(camel_to_snake("temperature"), "temperature");
        assert_eq!(camel_to_snake("topP"), "top_p");
        assert_eq!(camel_to_snake("topK"), "top_k");
        assert_eq!(camel_to_snake("maxTokens"), "max_tokens");
        assert_eq!(camel_to_snake("repeatPenalty"), "repeat_penalty");
        assert_eq!(camel_to_snake("presencePenalty"), "presence_penalty");
        assert_eq!(camel_to_snake("minP"), "min_p");
    }

    #[test]
    fn test_resolve_with_defaults_hierarchy() {
        let request = InferenceConfig {
            temperature: Some(0.9),
            ..Default::default()
        };
        let model = InferenceConfig {
            temperature: Some(0.5),
            top_p: Some(0.8),
            ..Default::default()
        };
        let global = InferenceConfig {
            top_k: Some(10),
            ..Default::default()
        };

        let resolved = request.resolve_with_defaults(
            Some(&model),
            Some(&global),
            ModelSamplingContext::default(),
        );

        assert_eq!(resolved.temperature, Some(0.9)); // request wins
        assert_eq!(resolved.top_p, Some(0.8)); // model fills in
        assert_eq!(resolved.top_k, Some(10)); // global fills in
        assert_eq!(resolved.max_tokens, None); // no layer sets it; stays unset
        // Deferred to llama.cpp since ADR 0003 — no layer named it and the
        // floor no longer restates upstream's own 1.0.
        assert_eq!(resolved.repeat_penalty, None);
    }

    #[test]
    fn test_resolve_with_defaults_no_layers() {
        let base = InferenceConfig::default();
        let resolved = base.resolve_with_defaults(None, None, ModelSamplingContext::default());
        // Should equal hardcoded defaults
        assert_eq!(resolved, InferenceConfig::with_hardcoded_defaults());
    }

    /// Every layer contributes exactly one distinguishable parameter, so a
    /// single assertion set pins the whole precedence ladder.
    #[test]
    fn test_resolve_with_profile_full_precedence_ladder() {
        let request = InferenceConfig {
            temperature: Some(0.9),
            ..Default::default()
        };
        let profile = InferenceConfig {
            temperature: Some(0.2),
            top_p: Some(0.85),
            ..Default::default()
        };
        let model = InferenceConfig {
            temperature: Some(0.5),
            top_p: Some(0.8),
            presence_penalty: Some(1.5),
            ..Default::default()
        };
        let global = InferenceConfig {
            top_k: Some(10),
            ..Default::default()
        };

        let resolved = request.resolve_with_profile(
            Some(&profile),
            Some(&model),
            Some(&global),
            ModelSamplingContext::default(),
        );

        assert_eq!(resolved.temperature, Some(0.9)); // request beats profile
        assert_eq!(resolved.top_p, Some(0.85)); // profile beats model
        assert_eq!(resolved.top_k, Some(10)); // global fills in
        // The request claimed the temperature, so the model's 1.5 — tuned for
        // its own 0.5 — must not fall through. Nothing is sent instead: the
        // neutral floor used to restate upstream's 0.0 here and ADR 0003
        // deferred it, so llama.cpp supplies the same number it always did.
        // The suppression is still visible in the provenance, which reports
        // `FloorCoupled` rather than a plain absence.
        assert_eq!(resolved.presence_penalty, None);
        assert_eq!(resolved.repeat_penalty, None);
    }

    /// The invariant that makes one global profile safe across differing
    /// architectures: parameters the profile leaves `None` still resolve from
    /// the model, so selecting a profile cannot erase per-model tuning.
    ///
    /// The exception is parameters tuned against temperature — see
    /// [`test_profile_temperature_does_not_inherit_model_penalties`].
    #[test]
    fn test_sparse_profile_does_not_erase_model_defaults() {
        let profile = InferenceConfig {
            temperature: Some(0.2),
            ..Default::default()
        };
        let model = InferenceConfig::reasoning_profile();

        let resolved = InferenceConfig::default().resolve_with_profile(
            Some(&profile),
            Some(&model),
            None,
            ModelSamplingContext::default(),
        );

        assert_eq!(resolved.temperature, Some(0.2)); // the profile's one opinion
        // Untuned parameters the profile stayed silent about still come from
        // the model — this is what keeps one profile safe across architectures.
        assert_eq!(resolved.top_k, model.top_k);
        assert_eq!(resolved.top_p, model.top_p);
        assert_eq!(resolved.max_tokens, model.max_tokens);
    }

    /// Regression for #621: a sparse profile that lowers the temperature must
    /// not inherit penalties the model tuned for a much broader distribution.
    ///
    /// `reasoning_profile()` pairs `temperature 1.0` with `presence_penalty
    /// 1.5` deliberately. Applying that 1.5 to a near-greedy `temperature 0.2`
    /// request is a recipe no layer ever intended, and it reached production on
    /// every `:coding` request.
    ///
    /// The #621 fix originally floored `presence_penalty` to the universal
    /// neutral `0.0` here — correct in that it stopped the wrong transplant,
    /// but it also zeroed the model's only anti-repetition guard on a
    /// reasoning model, which is a second failure mode of its own (see the
    /// 2026-07-31 incident this floor was added for). `model_is_reasoning:
    /// true` selects [`InferenceConfig::reasoning_floor`] instead, which keeps
    /// a real, non-tuned-for-0.2 guard in place.
    #[test]
    fn test_profile_temperature_does_not_inherit_model_penalties() {
        let model = InferenceConfig::reasoning_profile();
        assert_eq!(model.temperature, Some(1.0), "guards the premise");
        assert_eq!(model.presence_penalty, Some(1.5), "guards the premise");

        // Mirrors the shipped `coding` profile.
        let profile = InferenceConfig {
            temperature: Some(0.2),
            top_p: Some(0.95),
            top_k: Some(20),
            max_tokens: Some(8192),
            min_p: Some(0.05),
            ..Default::default()
        };

        let resolved = InferenceConfig::default().resolve_with_profile(
            Some(&profile),
            Some(&model),
            None,
            ModelSamplingContext {
                is_reasoning: true,
                ..Default::default()
            },
        );

        assert_eq!(resolved.temperature, Some(0.2));
        assert_eq!(
            resolved.presence_penalty,
            Some(1.0),
            "must not inherit 1.5, but must not go silently to zero either"
        );
        assert_eq!(
            resolved.repeat_penalty, None,
            "not the model's 1.2, and no longer restated at the floor either"
        );
        assert_eq!(resolved.min_p, Some(0.05), "the profile's own value stands");
    }

    /// The coupling is directional: a layer that supplies a temperature *and*
    /// its penalties still contributes them together, so a coherent recipe
    /// stored on a model is untouched when nothing above it sets a temperature.
    #[test]
    fn test_model_recipe_applies_intact_when_no_layer_sets_temperature() {
        let model = InferenceConfig::reasoning_profile();
        // A profile with opinions only about untuned parameters.
        let profile = InferenceConfig {
            top_k: Some(64),
            ..Default::default()
        };

        let resolved = InferenceConfig::default().resolve_with_profile(
            Some(&profile),
            Some(&model),
            None,
            ModelSamplingContext {
                is_reasoning: true,
                ..Default::default()
            },
        );

        assert_eq!(resolved.temperature, model.temperature);
        assert_eq!(resolved.presence_penalty, model.presence_penalty);
        assert_eq!(resolved.repeat_penalty, model.repeat_penalty);
        assert_eq!(resolved.top_k, Some(64)); // profile still wins where it spoke
    }

    /// `resolve_with_defaults` delegates to `resolve_with_profile`, so the two
    /// must stay observably identical when no profile is selected.
    #[test]
    fn test_resolve_with_defaults_matches_profile_form_with_no_profile() {
        let request = InferenceConfig {
            temperature: Some(0.9),
            ..Default::default()
        };
        let model = InferenceConfig::reasoning_profile();
        let global = InferenceConfig {
            top_k: Some(10),
            ..Default::default()
        };

        assert_eq!(
            request.clone().resolve_with_defaults(
                Some(&model),
                Some(&global),
                ModelSamplingContext::default()
            ),
            request.resolve_with_profile(
                None,
                Some(&model),
                Some(&global),
                ModelSamplingContext::default()
            ),
        );
    }

    // ── Provenance agrees with the values ─────────────────────────────────

    /// Assert, for every field, that the reported source actually accounts for
    /// the resolved value.
    ///
    /// This is the invariant that makes the two impossible to drift apart, and
    /// it is the check that would have caught the `describe_provenance`
    /// divergence this API replaced: a `Layer(i)` claim is only true if that
    /// layer really carries the resolved value.
    /// Field name, the value that resolved, and how to read that field off any
    /// layer — enough to check a reported source against reality.
    type FieldCheck = (
        &'static str,
        Option<f32>,
        fn(&InferenceConfig) -> Option<f32>,
    );

    #[track_caller]
    fn assert_sources_explain_values(layers: &[Option<&InferenceConfig>], floor: &InferenceConfig) {
        let (resolved, sources) = InferenceConfig::resolve_layers_with_sources(layers, floor);

        let checks: [FieldCheck; 5] = [
            ("temperature", resolved.temperature, |c| c.temperature),
            ("top_p", resolved.top_p, |c| c.top_p),
            ("presence_penalty", resolved.presence_penalty, |c| {
                c.presence_penalty
            }),
            ("repeat_penalty", resolved.repeat_penalty, |c| {
                c.repeat_penalty
            }),
            ("min_p", resolved.min_p, |c| c.min_p),
        ];

        for (name, value, get) in checks {
            let source = sources
                .iter()
                .find(|(field, _)| *field == name)
                .expect("field is reported")
                .1;
            match source {
                ParamSource::Layer(i) => {
                    let layer = layers[i].expect("a named layer is populated");
                    assert_eq!(get(layer), value, "{name}: layer {i} must carry the value");
                }
                ParamSource::Floor | ParamSource::FloorCoupled => {
                    assert_eq!(get(floor), value, "{name}: must equal the floor");
                }
                ParamSource::Unset => assert_eq!(value, None, "{name}: must be absent"),
            }
        }
    }

    /// Across the shapes the tests above exercise individually, plus the
    /// coupling-rule cases, provenance must account for every resolved value.
    #[test]
    fn test_sources_always_account_for_the_resolved_values() {
        let sparse_profile = InferenceConfig {
            temperature: Some(0.2),
            ..Default::default()
        };
        let recipe = InferenceConfig::reasoning_profile();
        let penalty_only = InferenceConfig {
            presence_penalty: Some(1.2),
            ..Default::default()
        };
        let global = InferenceConfig {
            top_k: Some(10),
            min_p: Some(0.05),
            ..Default::default()
        };

        let ladders: [[Option<&InferenceConfig>; 4]; 6] = [
            // Nothing at all — everything falls to the floor.
            [None, None, None, None],
            // A sparse profile over a full recipe: the coupling rule fires.
            [None, Some(&sparse_profile), Some(&recipe), None],
            // The recipe alone, unclaimed from above.
            [None, None, Some(&recipe), None],
            // The drift case: a penalty above a temperature claim below it.
            [Some(&penalty_only), None, Some(&recipe), None],
            // No layer names a temperature — the trio gap-fills normally.
            [Some(&penalty_only), None, None, Some(&global)],
            // Every rung populated.
            [
                Some(&penalty_only),
                Some(&sparse_profile),
                Some(&recipe),
                Some(&global),
            ],
        ];

        for floor in [
            InferenceConfig::with_hardcoded_defaults(),
            InferenceConfig::reasoning_floor(),
        ] {
            for ladder in &ladders {
                assert_sources_explain_values(ladder, &floor);
            }
        }
    }

    /// `max_tokens` is the one parameter with no floor value, so an untouched
    /// ladder reports it as genuinely unset rather than as a floor default.
    #[test]
    fn test_max_tokens_reports_unset_rather_than_floor() {
        let (_, sources) = InferenceConfig::resolve_layers_with_sources(
            &[None],
            &InferenceConfig::with_hardcoded_defaults(),
        );
        assert_eq!(sources.max_tokens, ParamSource::Unset);
        // `temperature` is now the only field with a floor to fall back on —
        // ADR 0003 deferred the other six, so they report as `Unset` for the
        // same reason `max_tokens` always has.
        assert_eq!(sources.temperature, ParamSource::Floor);
        assert_eq!(sources.top_p, ParamSource::Unset);
        assert_eq!(sources.min_p, ParamSource::Unset);
    }

    /// The two floor variants are distinguishable: a trio suppressed by the
    /// coupling rule must not look the same as one nobody ever set.
    #[test]
    fn test_coupled_suppression_is_distinguishable_from_plain_absence() {
        let claim = InferenceConfig {
            temperature: Some(0.2),
            ..Default::default()
        };
        let floor = InferenceConfig::with_hardcoded_defaults();

        let (_, claimed) = InferenceConfig::resolve_layers_with_sources(&[Some(&claim)], &floor);
        assert_eq!(claimed.presence_penalty, ParamSource::FloorCoupled);

        // Since ADR 0003 the neutral floor names no `presence_penalty`, so an
        // untouched one is a genuine absence rather than a floor value. The
        // distinction the test exists for is unaffected and now sharper: the
        // coupling rule is still reported, and "nobody set this" is still a
        // different answer from "the rule discarded something".
        let (_, untouched) = InferenceConfig::resolve_layers_with_sources(&[None], &floor);
        assert_eq!(untouched.presence_penalty, ParamSource::Unset);
        assert_ne!(claimed.presence_penalty, untouched.presence_penalty);

        // And a reasoning model, whose floor *does* name it, still reports the
        // plain floor — the two floors now differ in provenance, not only in
        // value.
        let (_, reasoning) = InferenceConfig::resolve_layers_with_sources(
            &[None],
            &InferenceConfig::reasoning_floor(),
        );
        assert_eq!(reasoning.presence_penalty, ParamSource::Floor);
    }

    /// `resolve_with_profile` delegates to the explained form, so the two must
    /// agree on the value, and the ladder indices must match `SamplingLayer`.
    #[test]
    fn test_resolve_with_profile_explained_matches_the_plain_form() {
        let profile = InferenceConfig {
            temperature: Some(0.2),
            ..Default::default()
        };
        let model = InferenceConfig::reasoning_profile();
        let ctx = ModelSamplingContext {
            is_reasoning: true,
            defaults_origin: Some(DefaultsOrigin::User),
        };

        let plain = InferenceConfig::default().resolve_with_profile(
            Some(&profile),
            Some(&model),
            None,
            ctx,
        );
        let (explained, sources) = InferenceConfig::default().resolve_with_profile_explained(
            Some(&profile),
            Some(&model),
            None,
            ctx,
        );

        assert_eq!(plain, explained);
        // The profile sits at rung 1, and a user-set model at rung 2.
        assert_eq!(sources.temperature, ParamSource::Layer(1));
        assert_eq!(
            crate::domain::SamplingLayer::from_index(1),
            Some(crate::domain::SamplingLayer::Profile)
        );
        assert_eq!(sources.top_k, ParamSource::Layer(2));
        assert_eq!(
            crate::domain::SamplingLayer::from_index(2),
            Some(crate::domain::SamplingLayer::ModelUserSet)
        );
    }

    /// An auto-detected recipe drops to rung 4, below global settings — the
    /// #685 ranking, now visible in the provenance rather than only in values.
    #[test]
    fn test_an_auto_detected_recipe_reports_the_lower_rung() {
        let model = InferenceConfig::reasoning_profile();
        let global = InferenceConfig {
            top_k: Some(10),
            ..Default::default()
        };
        let ctx = ModelSamplingContext {
            is_reasoning: true,
            defaults_origin: Some(DefaultsOrigin::AutoDetected),
        };

        let (_, sources) = InferenceConfig::default().resolve_with_profile_explained(
            None,
            Some(&model),
            Some(&global),
            ctx,
        );

        assert_eq!(sources.top_k, ParamSource::Layer(3), "global wins top_k");
        assert_eq!(
            sources.temperature,
            ParamSource::Layer(4),
            "the auto-detected recipe sits below global"
        );
    }

    #[test]
    fn test_openai_json_roundtrip() {
        let config = InferenceConfig {
            temperature: Some(0.7),
            top_p: Some(0.9),
            repeat_penalty: Some(1.1),
            ..Default::default()
        };
        let patch = config.to_openai_json_patch();

        // snake_case keys present for Some fields
        assert!(patch.contains_key("temperature"));
        assert!(patch.contains_key("top_p"));
        assert!(patch.contains_key("repeat_penalty"));
        // None fields absent
        assert!(!patch.contains_key("top_k"));
        assert!(!patch.contains_key("max_tokens"));

        // Roundtrip via from_openai_json
        let val = serde_json::Value::Object(patch);
        let back = InferenceConfig::from_openai_json(&val);
        assert_eq!(back.temperature, Some(0.7));
        assert_eq!(back.top_p, Some(0.9));
        assert_eq!(back.repeat_penalty, Some(1.1));
        assert!(back.top_k.is_none());
    }

    #[test]
    fn test_from_openai_json_unknown_fields_ignored() {
        let val = serde_json::json!({
            "temperature": 0.5,
            "model": "llama3",
            "messages": []
        });
        let config = InferenceConfig::from_openai_json(&val);
        assert_eq!(config.temperature, Some(0.5));
        assert!(config.top_p.is_none());
    }

    // ── Client sampling extraction ────────────────────────────────────────

    /// **The defect this was written for.** The old implementation
    /// camel-cased the whole body, deserialised it as one struct and called
    /// `.unwrap_or_default()`, so a single unreadable key returned an
    /// all-`None` config and the client's other ten values vanished with it.
    #[test]
    fn one_unreadable_field_does_not_cost_the_other_ten() {
        let val = serde_json::json!({
            "temperature": 0.2,
            "top_p": 0.9,
            "top_k": 30,
            "max_tokens": "not a number",   // the offender
            "repeat_penalty": 1.1,
            "presence_penalty": 0.3,
            "min_p": 0.02,
            "dry_multiplier": 0.8,
            "dry_base": 1.75,
            "dry_allowed_length": 2,
            "dry_penalty_last_n": 64,
        });

        let (cfg, issues) = InferenceConfig::extract_client_sampling(&val);

        assert_eq!(cfg.max_tokens, None, "the bad field is dropped");
        assert_eq!(
            issues.len(),
            1,
            "and only that field is reported: {issues:?}"
        );

        assert_eq!(cfg.temperature, Some(0.2));
        assert_eq!(cfg.top_p, Some(0.9));
        assert_eq!(cfg.top_k, Some(30));
        assert_eq!(cfg.repeat_penalty, Some(1.1));
        assert_eq!(cfg.presence_penalty, Some(0.3));
        assert_eq!(cfg.min_p, Some(0.02));
        assert_eq!(cfg.dry_multiplier, Some(0.8));
        assert_eq!(cfg.dry_base, Some(1.75));
        assert_eq!(cfg.dry_allowed_length, Some(2));
        assert_eq!(cfg.dry_penalty_last_n, Some(64));
    }

    /// llama.cpp answers 200 to this, so gglib accepting it is the whole
    /// point — before, it was the trip case that discarded the layer.
    /// ADR 0003 finding 6.
    #[test]
    fn max_tokens_minus_one_means_no_limit() {
        let val = serde_json::json!({ "max_tokens": -1, "temperature": 0.4 });
        let (cfg, issues) = InferenceConfig::extract_client_sampling(&val);

        assert_eq!(cfg.max_tokens, None, "-1 is the wire spelling of unset");
        assert_eq!(cfg.temperature, Some(0.4), "and the rest still lands");
        assert!(
            matches!(issues.as_slice(), [FieldIssue::Normalised { field, .. }] if *field == "max_tokens"),
            "reported as normalised, not rejected: {issues:?}"
        );
    }

    /// Some clients emit every number as a float. llama.cpp takes it.
    #[test]
    fn an_integer_valued_float_is_accepted_for_an_integer_field() {
        let val = serde_json::json!({ "top_k": 40.0 });
        let (cfg, issues) = InferenceConfig::extract_client_sampling(&val);
        assert_eq!(cfg.top_k, Some(40));
        assert!(matches!(issues.as_slice(), [FieldIssue::Normalised { .. }]));
    }

    /// A float that would lose information is not the same case.
    #[test]
    fn a_fractional_float_is_rejected_for_an_integer_field() {
        let val = serde_json::json!({ "top_k": 40.5 });
        let (cfg, issues) = InferenceConfig::extract_client_sampling(&val);
        assert_eq!(cfg.top_k, None);
        assert!(matches!(issues.as_slice(), [FieldIssue::Rejected { .. }]));
    }

    /// llama.cpp answers 400 to this, so gglib rejects it too rather than
    /// quietly parsing a client bug into a working request.
    #[test]
    fn a_numeric_string_is_rejected_not_coerced() {
        let val = serde_json::json!({ "temperature": "0.7", "top_p": 0.9 });
        let (cfg, issues) = InferenceConfig::extract_client_sampling(&val);
        assert_eq!(cfg.temperature, None);
        assert_eq!(cfg.top_p, Some(0.9), "one bad field, one casualty");
        assert!(
            matches!(issues.as_slice(), [FieldIssue::Rejected { field, .. }] if *field == "temperature")
        );
    }

    /// An explicit `null` is a client saying nothing, not a client erring —
    /// several of them send it for every parameter they leave at default.
    #[test]
    fn an_explicit_null_is_silence_rather_than_an_issue() {
        let val = serde_json::json!({ "temperature": null, "top_k": null });
        let (cfg, issues) = InferenceConfig::extract_client_sampling(&val);
        assert_eq!(cfg.temperature, None);
        assert_eq!(cfg.top_k, None);
        assert!(issues.is_empty(), "no issue reported: {issues:?}");
    }

    /// The two halves have to agree, or a value gglib emits is a value gglib
    /// cannot read back — which is how a round-trip through the pipeline
    /// would quietly lose a field.
    #[test]
    fn to_patch_then_extract_is_the_identity() {
        let original = InferenceConfig {
            temperature: Some(0.35),
            top_p: Some(0.9),
            top_k: Some(30),
            max_tokens: Some(2048),
            repeat_penalty: Some(1.05),
            presence_penalty: Some(1.5),
            min_p: Some(0.05),
            dry_multiplier: Some(0.8),
            dry_base: Some(1.75),
            dry_allowed_length: Some(2),
            dry_penalty_last_n: Some(64),
        };

        let patch = serde_json::Value::Object(original.to_openai_json_patch());
        let (back, issues) = InferenceConfig::extract_client_sampling(&patch);

        assert_eq!(back, original);
        assert!(issues.is_empty(), "clean round trip: {issues:?}");
    }
}
