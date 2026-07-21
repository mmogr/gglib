//! Named, cross-model sampling profiles.
//!
//! A profile is a *named, sparse* [`InferenceConfig`] that a client selects per
//! request by appending `:{name}` to the model it asks for — `qwen3.6:coding`.
//! It exists because a single gglib proxy serves clients with incompatible
//! sampling needs: coding agents want low-temperature determinism while
//! conversational UIs want something warmer. Both hit the same model name, so
//! per-model `inference_defaults` alone cannot tell them apart.
//!
//! The `{name}:{variant}` shape follows the existing council virtual models
//! (`gglib-council:interactive`) and Ollama's universal `name:tag` convention,
//! which is what makes the variants render and select correctly in
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

use crate::domain::InferenceConfig;

/// Maximum length of a profile name.
///
/// Deliberately short. Profile names become part of the model id advertised to
/// clients (`{model}:{profile}`), and long ids are one of the reported causes
/// of model-id rejection in OpenAI-compatible frontends.
pub const MAX_PROFILE_NAME_LEN: usize = 32;

/// Names that cannot be used for a profile because they already mean something
/// as a `:{suffix}` on a model id.
///
/// These are the council virtual-model variants (`gglib-council:interactive`,
/// `gglib-council:native`). Those names are matched *whole* by the proxy before
/// any profile splitting happens, so a profile sharing a suffix with one would
/// never be reachable on that model — confusing rather than dangerous, but
/// worth rejecting at the point of creation instead of leaving the user to
/// discover it.
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
/// two parameters that actually characterise its use case, leaving everything
/// else to fall through to the model's own defaults.
///
/// `chat` is the only one listed in `/v1/models` out of the box — it is the
/// conversational-client case that motivates the feature, and one visible
/// variant keeps the model picker useful without swamping it.
#[must_use]
pub fn builtin_templates() -> Vec<InferenceProfile> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_lowercase_alphanumeric_and_hyphen() {
        for name in ["coding", "chat", "creative", "long-form", "gpt4-style", "a"] {
            assert!(validate_name(name).is_ok(), "should accept {name}");
        }
    }

    #[test]
    fn rejects_empty_name() {
        assert_eq!(validate_name(""), Err(ProfileNameError::Empty));
    }

    #[test]
    fn rejects_name_over_the_length_cap() {
        let long = "a".repeat(MAX_PROFILE_NAME_LEN + 1);
        assert_eq!(
            validate_name(&long),
            Err(ProfileNameError::TooLong(MAX_PROFILE_NAME_LEN + 1))
        );
        assert!(validate_name(&"a".repeat(MAX_PROFILE_NAME_LEN)).is_ok());
    }

    /// Uppercase, underscores, dots, spaces and colons are all outside the
    /// conservative set — the colon especially, since it is the delimiter.
    #[test]
    fn rejects_characters_outside_the_conservative_set() {
        for name in ["Coding", "long_form", "v1.2", "long form", "a:b", "café"] {
            assert!(
                matches!(
                    validate_name(name),
                    Err(ProfileNameError::InvalidCharacters(_))
                ),
                "should reject {name}"
            );
        }
    }

    #[test]
    fn rejects_leading_or_trailing_hyphen() {
        for name in ["-coding", "coding-", "-"] {
            assert!(
                matches!(
                    validate_name(name),
                    Err(ProfileNameError::HyphenBoundary(_))
                ),
                "should reject {name}"
            );
        }
    }

    #[test]
    fn rejects_council_virtual_model_suffixes() {
        for name in RESERVED_PROFILE_NAMES {
            assert_eq!(
                validate_name(name),
                Err(ProfileNameError::Reserved((*name).to_owned()))
            );
        }
    }

    #[test]
    fn builtin_template_names_are_valid_and_unique() {
        let templates = builtin_templates();
        let mut names: Vec<&str> = templates.iter().map(|p| p.name.as_str()).collect();
        names.sort_unstable();
        let unique_count = {
            let mut deduped = names.clone();
            deduped.dedup();
            deduped.len()
        };
        assert_eq!(names.len(), unique_count, "template names must be unique");

        for profile in &templates {
            assert!(profile.validate().is_ok(), "invalid name: {}", profile.name);
        }
    }

    /// The central invariant: a template must leave the parameters it does not
    /// care about as `None` so they still resolve from the model's own
    /// defaults. A template that filled every field would silently override
    /// per-model tuning such as `reasoning_profile`'s `presence_penalty`.
    #[test]
    fn builtin_templates_are_sparse() {
        for profile in builtin_templates() {
            let c = &profile.config;
            assert!(c.temperature.is_some(), "{} sets temperature", profile.name);
            assert!(c.top_p.is_some(), "{} sets top_p", profile.name);
            assert!(c.top_k.is_none(), "{} leaves top_k open", profile.name);
            assert!(
                c.max_tokens.is_none(),
                "{} leaves max_tokens open",
                profile.name
            );
            assert!(
                c.repeat_penalty.is_none(),
                "{} leaves repeat_penalty open",
                profile.name
            );
            assert!(
                c.presence_penalty.is_none(),
                "{} leaves presence_penalty open",
                profile.name
            );
            assert!(c.min_p.is_none(), "{} leaves min_p open", profile.name);
        }
    }

    #[test]
    fn serializes_with_camel_case_keys() {
        let profile = &builtin_templates()[0];
        let json = serde_json::to_value(profile).expect("serializes");
        assert!(json.get("listInModels").is_some());
        assert!(json.get("list_in_models").is_none());
    }
}
