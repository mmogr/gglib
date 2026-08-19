//! Named, cross-model sampling profiles.
//!
//! A profile is a *named, sparse* [`InferenceConfig`] that a client selects per
//! request by appending `:{name}` to the model it asks for — `qwen3.6:coding`.
//! It exists because a single gglib proxy serves clients with incompatible
//! sampling needs: coding agents want low-temperature determinism while
//! conversational UIs want something warmer. Both hit the same model name, so
//! per-model `inference_defaults` alone cannot tell them apart.
//!
//! The `{name}:{variant}` shape follows Ollama's universal `name:tag`
//! convention, which is what makes the variants render and select correctly in
//! OpenAI-compatible clients like `OpenWebUI`.
//!
//! # Profiles are sparse
//!
//! Only the fields a profile explicitly sets are `Some`; the rest stay `None`
//! and fall through to the layers below (per-model defaults, then global
//! settings, then the hardcoded fallback). This is what makes one global
//! profile safe to apply across heterogeneous model architectures: a `coding`
//! profile that sets only `temperature` and `top_p` still lets a thinking model
//! contribute its own `presence_penalty` from
//! [`InferenceConfig::reasoning_profile`]. A profile that carried a value for
//! every field would silently erase per-model tuning that exists for good
//! architectural reasons.
//!
//! See [`InferenceConfig::resolve_with_profile`] for the full merge order.

use serde::{Deserialize, Serialize};

use crate::domain::{InferenceConfig, ReasoningEffort};

/// Maximum length of a profile name.
///
/// Deliberately short. Profile names become part of the model id advertised to
/// clients (`{model}:{profile}`), and long ids are one of the reported causes
/// of model-id rejection in OpenAI-compatible frontends.
pub const MAX_PROFILE_NAME_LEN: usize = 32;

/// Names that cannot be used for a profile because they already mean something
/// as a `:{suffix}` on a model id.
///
/// The list is held over from the removed council virtual models, whose
/// `:interactive` and `:native` suffixes the proxy matched whole. Nothing
/// claims these suffixes today, so the guard is a namespace reservation
/// rather than a correctness requirement — kept because it is user-visible
/// (the settings editor rejects them by name) and costs nothing.
pub const RESERVED_PROFILE_NAMES: &[&str] = &["interactive", "native"];

/// Why a profile name was rejected.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProfileNameError {
    #[error("profile name cannot be empty")]
    Empty,

    #[error("profile name is {0} characters; the maximum is {MAX_PROFILE_NAME_LEN}")]
    TooLong(usize),

    #[error(
        "profile name '{0}' contains invalid characters; use lowercase letters, digits, and '-'"
    )]
    InvalidCharacters(String),

    #[error("profile name '{0}' cannot start or end with '-'")]
    HyphenBoundary(String),

    #[error("profile name '{0}' is reserved")]
    Reserved(String),
}

/// A named sampling profile applied on top of a model's own defaults.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct InferenceProfile {
    /// Profile slug, used as the `:{suffix}` on a model id.
    ///
    /// Constrained by [`validate_name`] to lowercase alphanumerics and `-`.
    pub name: String,

    /// Human-readable summary, surfaced in `/v1/models` and the settings UI.
    pub description: Option<String>,

    /// The sampling overrides. Sparse — see the module docs.
    pub config: InferenceConfig,

    /// Whether to advertise `{model}:{name}` as its own `/v1/models` entry.
    ///
    /// Off by default: with several models and several profiles the full cross
    /// product would swamp a client's model picker. Users opt in for the one or
    /// two profiles they switch between often; the rest stay addressable by
    /// name without appearing in the list.
    pub list_in_models: bool,
}

/// Validate a profile name.
///
/// The accepted set — lowercase alphanumerics and `-`, 1–[`MAX_PROFILE_NAME_LEN`]
/// characters, no leading or trailing `-` — is deliberately narrower than what
/// most clients accept. Ollama-style `name:tag` ids prove that colons and
/// hyphens are safe in OpenAI-compatible frontends, but there are field reports
/// of ids containing underscores being rejected where the same id without one
/// worked. This set is the conservative intersection.
///
/// # Errors
///
/// Returns the specific [`ProfileNameError`] describing the first rule violated.
pub fn validate_name(name: &str) -> Result<(), ProfileNameError> {
    if name.is_empty() {
        return Err(ProfileNameError::Empty);
    }
    if name.len() > MAX_PROFILE_NAME_LEN {
        return Err(ProfileNameError::TooLong(name.len()));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(ProfileNameError::InvalidCharacters(name.to_owned()));
    }
    if name.starts_with('-') || name.ends_with('-') {
        return Err(ProfileNameError::HyphenBoundary(name.to_owned()));
    }
    if RESERVED_PROFILE_NAMES.contains(&name) {
        return Err(ProfileNameError::Reserved(name.to_owned()));
    }
    Ok(())
}

impl InferenceProfile {
    /// Validate this profile's name.
    ///
    /// # Errors
    ///
    /// Propagates [`validate_name`].
    pub fn validate(&self) -> Result<(), ProfileNameError> {
        validate_name(&self.name)
    }
}

/// Starting-point profiles a user can install and then edit.
///
/// These are *templates*, not behaviour: nothing reads them at request time and
/// installing them simply seeds the user's own profile list. Each sets only the
/// parameters that actually characterise its use case, leaving everything else
/// to fall through to the model's own defaults.
///
/// Two families, kept in separate functions because they are separate
/// arguments: [`sampling_templates`] picks a distribution, and
/// [`reasoning_templates`] picks how hard the model is asked to think.
#[must_use]
pub fn builtin_templates() -> Vec<InferenceProfile> {
    let mut templates = sampling_templates();
    templates.extend(reasoning_templates());
    templates
}

/// The distribution-shaping templates: temperature and `top_p` only.
///
/// `chat` is the only one listed in `/v1/models` out of the box — it is the
/// conversational-client case that motivates the feature, and one visible
/// variant keeps the model picker useful without swamping it.
fn sampling_templates() -> Vec<InferenceProfile> {
    vec![
        InferenceProfile {
            name: "coding".to_owned(),
            description: Some("Low-variance sampling for code generation and tool use.".to_owned()),
            config: InferenceConfig {
                temperature: Some(0.2),
                top_p: Some(0.9),
                ..Default::default()
            },
            list_in_models: false,
        },
        InferenceProfile {
            name: "chat".to_owned(),
            description: Some("Balanced sampling for conversational use.".to_owned()),
            config: InferenceConfig {
                temperature: Some(0.7),
                top_p: Some(0.95),
                ..Default::default()
            },
            list_in_models: true,
        },
        InferenceProfile {
            name: "creative".to_owned(),
            description: Some("Wider sampling for brainstorming and prose.".to_owned()),
            config: InferenceConfig {
                temperature: Some(1.1),
                top_p: Some(0.98),
                ..Default::default()
            },
            list_in_models: false,
        },
    ]
}

/// One template per rung of the [`ReasoningEffort`] ladder.
///
/// # Why each rung sets *both* controls
///
/// [`reasoning_effort`] is a string a chat template may read at render time —
/// and may equally ignore, in perfect silence (ADR 0007 finding 3). A profile
/// that carried only the effort level would therefore do *nothing at all* on
/// such a model, while reading in `gglib config profile show` as though it
/// had. Pairing it with [`reasoning_budget_tokens`] — which llama.cpp itself
/// enforces, whatever the template does — means the rung degrades to a
/// narrower promise rather than to no promise: on a template that reads the
/// variable the user gets both, and on one that does not they still get a
/// thinking cap they chose.
///
/// # The budget ladder, and why these numbers
///
/// | profile | effort | budget | what the budget is for |
/// |---------|--------|--------|------------------------|
/// | `minimal` | `minimal` | 256 | a sentence or two of scratch work — an answer, not a deliberation |
/// | `low` | `low` | 1024 | one short chain; enough to check an assumption |
/// | `medium` | `medium` | 4096 | the middle rung, and roughly what an untouched `gpt-oss` turn spends |
/// | `high` | `high` | 16384 | multi-step work where the thinking is the point |
/// | `xhigh` | `xhigh` | 32768 | long deliberation, still bounded so a loop terminates |
/// | `max` | `max` | -1 | defer to the launch-time `--reasoning-budget` |
///
/// Roughly a quadrupling per rung to 16384 and a doubling after, because the
/// levels are not linear either: nothing in llama.cpp compares them and a
/// template is free to treat two of them identically, so the ladder is spaced
/// widely enough that adjacent rungs are distinguishable in practice rather
/// than finely enough to imply a precision that does not exist. Nothing is
/// measured here — these are *starting points a user edits*, and the one
/// number that is not a guess is `max`'s `-1`, which declines to invent a
/// ceiling and leaves the operator's own launch default in charge.
///
/// # Only three are listed
///
/// Six listed variants per model would swamp the very model picker
/// [`InferenceProfile::list_in_models`] exists to protect, so `low`, `high`
/// and `max` — the ends and a usable middle — are the visible ones. The other
/// three stay fully usable by name as `<model>:minimal` and friends.
///
/// [`ReasoningEffort`]: crate::domain::ReasoningEffort
/// [`reasoning_effort`]: InferenceConfig::reasoning_effort
/// [`reasoning_budget_tokens`]: InferenceConfig::reasoning_budget_tokens
fn reasoning_templates() -> Vec<InferenceProfile> {
    /// `(name, effort, budget, listed)` — one row per rung, weakest first.
    const LADDER: [(&str, ReasoningEffort, i32, bool); 6] = [
        ("minimal", ReasoningEffort::Minimal, 256, false),
        ("low", ReasoningEffort::Low, 1024, true),
        ("medium", ReasoningEffort::Medium, 4096, false),
        ("high", ReasoningEffort::High, 16384, true),
        ("xhigh", ReasoningEffort::XHigh, 32768, false),
        ("max", ReasoningEffort::Max, -1, true),
    ];

    LADDER
        .into_iter()
        .map(|(name, effort, budget, listed)| InferenceProfile {
            name: name.to_owned(),
            description: Some(describe_rung(effort, budget)),
            config: InferenceConfig {
                reasoning_effort: Some(effort),
                reasoning_budget_tokens: Some(budget),
                ..Default::default()
            },
            list_in_models: listed,
        })
        .collect()
}

/// The description shown in `/v1/models` and the settings UI for one rung.
///
/// Spells out both halves, including the fact that the effort half is only a
/// request: a user reading the list should not have to know ADR 0007 to learn
/// that a template may ignore it.
fn describe_rung(effort: ReasoningEffort, budget: i32) -> String {
    let cap = if budget < 0 {
        "no gglib-set cap (defers to the launch default)".to_owned()
    } else {
        format!("at most {budget} thinking tokens")
    };
    format!("Asks for '{effort}' reasoning effort where the template reads it; {cap}.")
}

#[cfg(test)]
#[path = "inference_profile_tests.rs"]
mod inference_profile_tests;
