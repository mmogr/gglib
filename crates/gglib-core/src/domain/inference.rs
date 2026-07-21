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

    /// Convert inference config to llama CLI arguments.
    ///
    /// Returns a vector of argument strings suitable for passing to llama-server.
    /// Uses the same flag names as llama.cpp: `--temp`, `--top-p`, `--top-k`, `-n`, `--repeat-penalty`.
    ///
    /// This is the single source of truth for CLI flag conversion, used by:
    /// - `LlamaCommandBuilder` (for CLI commands)
    /// - GUI server startup (via `ServerConfig.extra_args`)
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
    /// # Example
    ///
    /// ```rust
    /// use gglib_core::domain::InferenceConfig;
    ///
    /// let request = InferenceConfig { temperature: Some(0.9), ..Default::default() };
    /// let model   = InferenceConfig { temperature: Some(0.5), top_p: Some(0.8), ..Default::default() };
    ///
    /// let resolved = request.resolve_with_defaults(Some(&model), None);
    /// assert_eq!(resolved.temperature, Some(0.9)); // request wins
    /// assert_eq!(resolved.top_p,       Some(0.8)); // model fills in
    /// assert_eq!(resolved.top_k,       Some(40));  // hardcoded fallback
    /// ```
    ///
    /// [`resolve_with_profile`]: Self::resolve_with_profile
    #[must_use]
    pub const fn resolve_with_defaults(self, model: Option<&Self>, global: Option<&Self>) -> Self {
        self.resolve_with_profile(None, model, global)
    }

    /// Resolve inference parameters using the full 5-level hierarchy.
    ///
    /// Applies fallback layers in order, with each layer filling only `None`
    /// fields from `self` — explicit values are never overwritten:
    ///
    /// 1. `self` — caller-supplied overrides (request params, CLI flags, etc.)
    /// 2. `profile` — the named profile the request selected, if any
    /// 3. `model` — per-model stored defaults
    /// 4. `global` — global settings defaults
    /// 5. [`with_hardcoded_defaults`] — compile-time fallback values
    ///
    /// This is the single source of truth for inference parameter resolution
    /// across every gglib surface; [`resolve_with_defaults`] delegates here so
    /// there is exactly one merge order to reason about and to test.
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
    /// # Example
    ///
    /// ```rust
    /// use gglib_core::domain::InferenceConfig;
    ///
    /// // A sparse profile: sets temperature, says nothing about anything else.
    /// let profile = InferenceConfig { temperature: Some(0.2), ..Default::default() };
    /// // A thinking model's stored defaults.
    /// let model = InferenceConfig {
    ///     temperature: Some(1.0),
    ///     presence_penalty: Some(1.5),
    ///     ..Default::default()
    /// };
    ///
    /// let resolved = InferenceConfig::default()
    ///     .resolve_with_profile(Some(&profile), Some(&model), None);
    ///
    /// assert_eq!(resolved.temperature,      Some(0.2)); // profile beats model
    /// assert_eq!(resolved.presence_penalty, Some(1.5)); // model still fills in
    /// ```
    ///
    /// [`with_hardcoded_defaults`]: Self::with_hardcoded_defaults
    /// [`resolve_with_defaults`]: Self::resolve_with_defaults
    #[must_use]
    pub const fn resolve_with_profile(
        mut self,
        profile: Option<&Self>,
        model: Option<&Self>,
        global: Option<&Self>,
    ) -> Self {
        if let Some(p) = profile {
            self.merge_with(p);
        }
        if let Some(m) = model {
            self.merge_with(m);
        }
        if let Some(g) = global {
            self.merge_with(g);
        }
        self.merge_with(&Self::with_hardcoded_defaults());
        self
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

    /// The two ways an unset `max_tokens` could still reach llama-server and
    /// cap generation: as a `max_tokens` key in the forwarded request body, or
    /// as a `-n` flag on the launch command line. `-n` is the more dangerous of
    /// the two — it sets `global_params.n_predict`, a server-wide ceiling that
    /// overrides even a per-request `-1`.
    #[test]
    fn test_unset_max_tokens_reaches_llama_server_by_neither_route() {
        let resolved = InferenceConfig::default().resolve_with_defaults(None, None);

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
        .resolve_with_defaults(None, None);

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

        let resolved = request.resolve_with_defaults(Some(&model), Some(&global));

        assert_eq!(resolved.temperature, Some(0.9)); // request wins
        assert_eq!(resolved.top_p, Some(0.8)); // model fills in
        assert_eq!(resolved.top_k, Some(10)); // global fills in
        assert_eq!(resolved.max_tokens, None); // no layer sets it; stays unset
        assert_eq!(resolved.repeat_penalty, Some(1.0)); // hardcoded fallback
    }

    #[test]
    fn test_resolve_with_defaults_no_layers() {
        let base = InferenceConfig::default();
        let resolved = base.resolve_with_defaults(None, None);
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

        let resolved = request.resolve_with_profile(Some(&profile), Some(&model), Some(&global));

        assert_eq!(resolved.temperature, Some(0.9)); // request beats profile
        assert_eq!(resolved.top_p, Some(0.85)); // profile beats model
        assert_eq!(resolved.presence_penalty, Some(1.5)); // model fills in
        assert_eq!(resolved.top_k, Some(10)); // global fills in
        assert_eq!(resolved.repeat_penalty, Some(1.0)); // hardcoded fallback
    }

    /// The invariant that makes one global profile safe across differing
    /// architectures: parameters the profile leaves `None` still resolve from
    /// the model, so selecting a profile cannot erase per-model tuning.
    #[test]
    fn test_sparse_profile_does_not_erase_model_defaults() {
        let profile = InferenceConfig {
            temperature: Some(0.2),
            ..Default::default()
        };
        let model = InferenceConfig::reasoning_profile();

        let resolved =
            InferenceConfig::default().resolve_with_profile(Some(&profile), Some(&model), None);

        assert_eq!(resolved.temperature, Some(0.2)); // the profile's one opinion
        // Everything the profile stayed silent about comes from the model.
        assert_eq!(resolved.presence_penalty, model.presence_penalty);
        assert_eq!(resolved.top_k, model.top_k);
        assert_eq!(resolved.top_p, model.top_p);
        assert_eq!(resolved.max_tokens, model.max_tokens);
        assert_eq!(resolved.min_p, model.min_p);
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
            request
                .clone()
                .resolve_with_defaults(Some(&model), Some(&global)),
            request.resolve_with_profile(None, Some(&model), Some(&global)),
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
