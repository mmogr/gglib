//! Inference configuration types.
//!
//! Defines shared types for configuring LLM inference parameters
//! (temperature, `top_p`, `top_k`, `max_tokens`, `repeat_penalty`,
//! `presence_penalty`, `min_p`).
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

use serde::{Deserialize, Serialize};

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
///     presence_penalty: None,
///     min_p: None,
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
    /// - 0.0: Disabled (explicit off; recommended by Qwen3.6)
    /// - 0.05: llama.cpp built-in default when the flag is omitted
    pub min_p: Option<f32>,
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

/// Convert a `snake_case` string to camelCase.
///
/// Inverse of [`camel_to_snake`]; used to normalise OpenAI-format body keys
/// (`top_p`, `max_tokens`, etc.) into the camelCase form expected by
/// `InferenceConfig`'s serde impl before deserialisation.
fn snake_to_camel(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut cap = false;
    for ch in s.chars() {
        if ch == '_' {
            cap = true;
        } else if cap {
            out.push(ch.to_ascii_uppercase());
            cap = false;
        } else {
            out.push(ch);
        }
    }
    out
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
    }

    /// Resolve an ordered list of sampling layers (highest priority first)
    /// into a single fully-resolved config, then fill anything still unset
    /// from `floor`.
    ///
    /// This is the one fold every multi-layer resolution surface goes
    /// through: [`resolve_with_profile`] wraps it for the simple
    /// request/profile/model/global shape, and
    /// [`crate::request_pipeline::sampling`] builds its own five-layer
    /// (cli/client/profile/model/global) array and calls it directly. There
    /// is exactly one place that decides what "wins" means.
    ///
    /// # Uncoupled parameters
    ///
    /// `top_p`, `top_k`, and `max_tokens` gap-fill independently: each takes
    /// the first `Some` value found scanning the layers top to bottom.
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
    /// tuned against anything, so the coupled trio gap-fills normally, exactly
    /// like the uncoupled parameters.
    ///
    /// [`resolve_with_profile`]: Self::resolve_with_profile
    /// [`reasoning_profile`]: Self::reasoning_profile
    #[must_use]
    pub fn resolve_layers(layers: &[Option<&Self>], floor: &Self) -> Self {
        let mut result = Self::default();

        for layer in layers.iter().flatten() {
            if result.top_p.is_none() {
                result.top_p = layer.top_p;
            }
            if result.top_k.is_none() {
                result.top_k = layer.top_k;
            }
            if result.max_tokens.is_none() {
                result.max_tokens = layer.max_tokens;
            }
        }

        result.temperature = layers.iter().flatten().find_map(|l| l.temperature);

        if let Some(claim) = layers.iter().flatten().find(|l| l.temperature.is_some()) {
            result.repeat_penalty = claim.repeat_penalty;
            result.presence_penalty = claim.presence_penalty;
            result.min_p = claim.min_p;
        } else {
            for layer in layers.iter().flatten() {
                if result.repeat_penalty.is_none() {
                    result.repeat_penalty = layer.repeat_penalty;
                }
                if result.presence_penalty.is_none() {
                    result.presence_penalty = layer.presence_penalty;
                }
                if result.min_p.is_none() {
                    result.min_p = layer.min_p;
                }
            }
        }

        result.merge_with(floor);
        result
    }

    /// Create a new config with all fields set to sensible defaults.
    ///
    /// These are the hardcoded fallback values used when no other
    /// defaults are configured.
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
    /// [`reasoning_profile`]: Self::reasoning_profile
    #[must_use]
    pub const fn with_hardcoded_defaults() -> Self {
        Self {
            temperature: Some(0.7),
            top_p: Some(0.95),
            top_k: Some(40),
            max_tokens: None,
            repeat_penalty: Some(1.0),
            presence_penalty: Some(0.0),
            min_p: Some(0.0),
        }
    }

    /// The coupled-trio floor for models tagged `reasoning`.
    ///
    /// [`resolve_layers`] falls back to a floor once it has decided which
    /// layer (if any) claims the coupled trio and that layer left a field
    /// unset. [`with_hardcoded_defaults`]'s neutral `presence_penalty: 0.0` is
    /// the right floor for most models, but wrong for a `reasoning`-tagged
    /// one: those degrade under greedy or near-greedy decoding into
    /// repetitive reasoning loops (see [`reasoning_profile`], which pairs
    /// `presence_penalty: 1.5` with `temperature: 1.0` specifically to
    /// prevent this). `1.0` keeps a real guard in place at the floor without
    /// asserting the full recipe tuned for a different temperature.
    ///
    /// [`resolve_layers`]: Self::resolve_layers
    /// [`with_hardcoded_defaults`]: Self::with_hardcoded_defaults
    /// [`reasoning_profile`]: Self::reasoning_profile
    #[must_use]
    pub const fn reasoning_floor() -> Self {
        Self {
            presence_penalty: Some(1.0),
            ..Self::with_hardcoded_defaults()
        }
    }

    /// Convert inference config to llama CLI arguments.
    ///
    /// Returns a vector of argument strings suitable for passing to llama-server.
    /// Uses the same flag names as llama.cpp: `--temp`, `--top-p`, `--top-k`, `-n`, `--repeat-penalty`.
    ///
    /// This is the single source of truth for CLI flag conversion, reached by
    /// every launch surface through `build_server_config` and
    /// `ServerConfig.extra_args`.
    ///
    /// # Example
    ///
    /// ```rust
    /// use gglib_core::domain::InferenceConfig;
    ///
    /// let config = InferenceConfig {
    ///     temperature: Some(0.8),
    ///     top_p: Some(0.9),
    ///     top_k: None,
    ///     max_tokens: Some(1024),
    ///     repeat_penalty: None,
    ///     presence_penalty: None,
    ///     min_p: None,
    /// };
    ///
    /// let args = config.to_cli_args();
    /// assert_eq!(args, vec!["--temp", "0.8", "--top-p", "0.9", "-n", "1024"]);
    /// ```
    #[must_use]
    pub fn to_cli_args(&self) -> Vec<String> {
        let mut args = Vec::new();

        if let Some(temp) = self.temperature {
            args.push("--temp".to_string());
            args.push(temp.to_string());
        }
        if let Some(top_p) = self.top_p {
            args.push("--top-p".to_string());
            args.push(top_p.to_string());
        }
        if let Some(top_k) = self.top_k {
            args.push("--top-k".to_string());
            args.push(top_k.to_string());
        }
        if let Some(max_tokens) = self.max_tokens {
            args.push("-n".to_string());
            args.push(max_tokens.to_string());
        }
        if let Some(repeat_penalty) = self.repeat_penalty {
            args.push("--repeat-penalty".to_string());
            args.push(repeat_penalty.to_string());
        }
        if let Some(presence_penalty) = self.presence_penalty {
            args.push("--presence-penalty".to_string());
            args.push(presence_penalty.to_string());
        }
        if let Some(min_p) = self.min_p {
            args.push("--min-p".to_string());
            args.push(min_p.to_string());
        }

        args
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
    /// assert_eq!(resolved.top_k,       Some(40));  // hardcoded fallback
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
    /// [`crate::request_pipeline::sampling`] needs a seventh layer (the
    /// client's own request, sitting between `self` and `profile`) and calls
    /// the underlying [`resolve_layers`] directly for that reason — the merge
    /// semantics are identical either way.
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
        let floor = if model_ctx.is_reasoning {
            Self::reasoning_floor()
        } else {
            Self::with_hardcoded_defaults()
        };
        let (user_model, auto_model) = match model_ctx.defaults_origin {
            Some(DefaultsOrigin::AutoDetected) => (None, model),
            _ => (model, None),
        };
        Self::resolve_layers(
            &[Some(&self), profile, user_model, global, auto_model],
            &floor,
        )
    }

    /// Parse inference parameters from an OpenAI-format JSON body (`snake_case` keys).
    ///
    /// Converts wire-format `snake_case` field names (`top_p`, `max_tokens`,
    /// `repeat_penalty`, etc.) to the internal camelCase representation via
    /// [`snake_to_camel`], then deserialises using the existing `serde` impl.
    /// Unknown or missing fields default to `None`.
    ///
    /// This is the inverse of [`to_openai_json_patch`].
    ///
    /// [`to_openai_json_patch`]: Self::to_openai_json_patch
    #[must_use]
    pub fn from_openai_json(value: &serde_json::Value) -> Self {
        let Some(obj) = value.as_object() else {
            return Self::default();
        };
        let camel: serde_json::Map<String, serde_json::Value> = obj
            .iter()
            .map(|(k, v)| (snake_to_camel(k), v.clone()))
            .collect();
        serde_json::from_value(serde_json::Value::Object(camel)).unwrap_or_default()
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

    #[test]
    fn test_hardcoded_defaults() {
        let config = InferenceConfig::with_hardcoded_defaults();
        assert_eq!(config.temperature, Some(0.7));
        assert_eq!(config.top_p, Some(0.95));
        assert_eq!(config.top_k, Some(40));
        // Deliberately absent: a fallback here would cap every request that
        // did not name its own. See `with_hardcoded_defaults`.
        assert_eq!(config.max_tokens, None);
        assert_eq!(config.repeat_penalty, Some(1.0));
        assert_eq!(config.presence_penalty, Some(0.0));
        assert_eq!(config.min_p, Some(0.0));
    }

    /// The reasoning floor differs from the hardcoded floor in exactly one
    /// field — a real anti-repetition guard where the neutral floor has none.
    #[test]
    fn test_reasoning_floor_differs_only_in_presence_penalty() {
        let neutral = InferenceConfig::with_hardcoded_defaults();
        let reasoning = InferenceConfig::reasoning_floor();

        assert_eq!(reasoning.presence_penalty, Some(1.0));
        assert_ne!(reasoning.presence_penalty, neutral.presence_penalty);

        assert_eq!(reasoning.temperature, neutral.temperature);
        assert_eq!(reasoning.top_p, neutral.top_p);
        assert_eq!(reasoning.top_k, neutral.top_k);
        assert_eq!(reasoning.max_tokens, neutral.max_tokens);
        assert_eq!(reasoning.repeat_penalty, neutral.repeat_penalty);
        assert_eq!(reasoning.min_p, neutral.min_p);
    }

    /// If nothing in the stack ever declares a temperature, nothing has been
    /// "tuned" against anything — the coupled trio must gap-fill exactly like
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

    /// The two ways an unset `max_tokens` could still reach llama-server and
    /// cap generation: as a `max_tokens` key in the forwarded request body, or
    /// as a `-n` flag on the launch command line. `-n` is the more dangerous of
    /// the two — it sets `global_params.n_predict`, a server-wide ceiling that
    /// overrides even a per-request `-1`.
    #[test]
    fn test_unset_max_tokens_reaches_llama_server_by_neither_route() {
        let resolved = InferenceConfig::default().resolve_with_defaults(
            None,
            None,
            ModelSamplingContext::default(),
        );

        assert!(
            !resolved.to_openai_json_patch().contains_key("max_tokens"),
            "an unset max_tokens must not be written into the request body"
        );
        assert!(
            !resolved.to_cli_args().contains(&"-n".to_string()),
            "an unset max_tokens must not become a server-wide -n ceiling"
        );
    }

    /// An explicit value must still travel by both routes — this change removes
    /// the *fallback*, not the parameter.
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
        let args = resolved.to_cli_args();
        let n_index = args.iter().position(|a| a == "-n").expect("-n emitted");
        assert_eq!(args[n_index + 1], "512");
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
    fn test_snake_to_camel() {
        assert_eq!(snake_to_camel("temperature"), "temperature");
        assert_eq!(snake_to_camel("top_p"), "topP");
        assert_eq!(snake_to_camel("top_k"), "topK");
        assert_eq!(snake_to_camel("max_tokens"), "maxTokens");
        assert_eq!(snake_to_camel("repeat_penalty"), "repeatPenalty");
        assert_eq!(snake_to_camel("presence_penalty"), "presencePenalty");
        assert_eq!(snake_to_camel("min_p"), "minP");
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
        assert_eq!(resolved.repeat_penalty, Some(1.0)); // hardcoded fallback
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
        // its own 0.5 — must not fall through. Neutral hardcoded value instead.
        assert_eq!(resolved.presence_penalty, Some(0.0));
        assert_eq!(resolved.repeat_penalty, Some(1.0)); // hardcoded fallback
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
            resolved.repeat_penalty,
            Some(1.0),
            "neutral, not the model's"
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
}
