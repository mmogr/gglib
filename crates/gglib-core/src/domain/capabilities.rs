//! Model capability detection and inference.
//!
//! Capabilities describe model constraints derived from chat templates.
//! Absence of a capability MUST NOT trigger behavior changes.
//!
//! # Invariant
//!
//! Message rewriting is only permitted when model capabilities explicitly
//! forbid the current message structure. Default behavior is pass-through.

use bitflags::bitflags;
use serde::{Deserialize, Serialize};

bitflags! {
    /// Model capabilities inferred from chat template analysis.
    ///
    /// These flags describe what the model's chat template can handle.
    /// Absence means "we don't know" or "not needed", not "forbidden".
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    #[repr(transparent)]
    pub struct ModelCapabilities: u32 {
        /// Model supports system role natively in its chat template.
        ///
        /// When set: system messages can be passed through unchanged.
        /// When unset: system messages must be converted to user messages.
        const SUPPORTS_SYSTEM_ROLE    = 0b0000_0001;

        /// Model requires strict user/assistant alternation.
        ///
        /// When set: consecutive messages of same role must be merged.
        /// When unset: message order can be arbitrary (OpenAI-style).
        const REQUIRES_STRICT_TURNS   = 0b0000_0010;

        /// Model supports tool/function calling.
        ///
        /// When set: tool_calls and tool role messages are supported.
        /// When unset: tool functionality should not be used.
        const SUPPORTS_TOOL_CALLS     = 0b0000_0100;

        /// Model has reasoning/thinking capability.
        ///
        /// When set: model may produce <think> tags or reasoning_content.
        /// When unset: model produces only standard responses.
        const SUPPORTS_REASONING      = 0b0000_1000;
    }
}

impl Default for ModelCapabilities {
    /// Default capabilities assume OpenAI-style chat models.
    ///
    /// This is the safe default for models without explicit templates,
    /// preserving standard instruction-following behavior.
    fn default() -> Self {
        Self::SUPPORTS_SYSTEM_ROLE
    }
}

impl Serialize for ModelCapabilities {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.bits().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ModelCapabilities {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let bits = u32::deserialize(deserializer)?;
        Ok(Self::from_bits_truncate(bits))
    }
}

impl ModelCapabilities {
    /// Check if model supports system role.
    pub const fn supports_system_role(self) -> bool {
        self.contains(Self::SUPPORTS_SYSTEM_ROLE)
    }

    /// Check if model requires strict user/assistant alternation.
    pub const fn requires_strict_turns(self) -> bool {
        self.contains(Self::REQUIRES_STRICT_TURNS)
    }

    /// Check if model supports tool/function calls.
    pub const fn supports_tool_calls(self) -> bool {
        self.contains(Self::SUPPORTS_TOOL_CALLS)
    }

    /// Check if model supports reasoning phases.
    pub const fn supports_reasoning(self) -> bool {
        self.contains(Self::SUPPORTS_REASONING)
    }
}

/// Infer model capabilities from chat template Jinja source.
///
/// Uses string heuristics to detect template constraints. Returns safe
/// defaults if template is missing or unparseable.
///
/// # Detection Strategy
///
/// - System role: Looks for explicit rejection messages in template
/// - Strict turns: Looks for alternation enforcement logic
/// - Tools/reasoning: Can be extended with similar patterns
///
/// # Fallback Behavior
///
/// Missing or unparseable templates default to OpenAI-style (system supported).
/// This prevents silent degradation of instruction-following.
pub fn infer_from_chat_template(template: Option<&str>) -> ModelCapabilities {
    let Some(template) = template else {
        // Missing template: assume OpenAI-style
        return ModelCapabilities::default();
    };

    let mut caps = ModelCapabilities::empty();

    // Check for system role restrictions
    // Mistral-style templates explicitly reject system role in error messages
    let forbids_system = template.contains("Only user, assistant and tool roles are supported")
        || template.contains("got system")
        || template.contains("Raise exception for unsupported roles");

    if forbids_system {
        // Absence of SUPPORTS_SYSTEM_ROLE means transformation required
    } else {
        caps |= ModelCapabilities::SUPPORTS_SYSTEM_ROLE;
    }

    // Check for strict alternation requirements
    // Mistral-style templates enforce user/assistant alternation with modulo checks
    let requires_alternation = template.contains("must alternate user and assistant")
        || template.contains("conversation roles must alternate")
        || template.contains("ns.index % 2");

    if requires_alternation {
        caps |= ModelCapabilities::REQUIRES_STRICT_TURNS;
    }

    // TODO: Add tool support detection (check for tool_calls handling)
    // TODO: Add reasoning detection (check for <think> or reasoning_content)

    caps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_capabilities() {
        let caps = ModelCapabilities::default();
        assert!(caps.supports_system_role());
        assert!(!caps.requires_strict_turns());
    }

    #[test]
    fn test_infer_openai_style() {
        let template = r#"
            {% for message in messages %}
                {{ message.role }}: {{ message.content }}
            {% endfor %}
        "#;
        let caps = infer_from_chat_template(Some(template));
        assert!(caps.supports_system_role());
        assert!(!caps.requires_strict_turns());
    }

    #[test]
    fn test_infer_mistral_style() {
        let template = r#"
            {% if message.role == 'system' %}
                {{ raise_exception('Only user, assistant and tool roles are supported, got system.') }}
            {% endif %}
            {% if (message['role'] == 'user') != (ns.index % 2 == 0) %}
                {{ raise_exception('conversation roles must alternate user and assistant') }}
            {% endif %}
        "#;
        let caps = infer_from_chat_template(Some(template));
        assert!(!caps.supports_system_role());
        assert!(caps.requires_strict_turns());
    }

    #[test]
    fn test_infer_missing_template() {
        let caps = infer_from_chat_template(None);
        assert!(caps.supports_system_role()); // Safe default
    }
}
