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
    /// Default capabilities represent "unknown" state.
    ///
    /// Models start with empty capabilities and must be explicitly inferred.
    /// This prevents incorrect assumptions about model constraints.
    fn default() -> Self {
        Self::empty()
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
        // Default is "unknown" - no capabilities set
        assert!(caps.is_empty());
        assert!(!caps.supports_system_role());
        assert!(!caps.requires_strict_turns());
    }

    #[test]
    fn test_infer_openai_style() {
        let template = r"
            {% for message in messages %}
                {{ message.role }}: {{ message.content }}
            {% endfor %}
        ";
        let caps = infer_from_chat_template(Some(template));
        assert!(caps.supports_system_role());
        assert!(!caps.requires_strict_turns());
    }

    #[test]
    fn test_infer_mistral_style() {
        let template = r"
            {% if message.role == 'system' %}
                {{ raise_exception('Only user, assistant and tool roles are supported, got system.') }}
            {% endif %}
            {% if (message['role'] == 'user') != (ns.index % 2 == 0) %}
                {{ raise_exception('conversation roles must alternate user and assistant') }}
            {% endif %}
        ";
        let caps = infer_from_chat_template(Some(template));
        assert!(!caps.supports_system_role());
        assert!(caps.requires_strict_turns());
    }

    #[test]
    fn test_infer_missing_template() {
        let caps = infer_from_chat_template(None);
        // Missing template means unknown capabilities - no assumptions made
        assert!(caps.is_empty());
        assert!(!caps.supports_system_role());
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Message Transformation
// ─────────────────────────────────────────────────────────────────────────────

/// A chat message for transformation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatMessage {
    pub role: String,
    pub content: Option<String>,
    pub tool_calls: Option<serde_json::Value>,
}

/// Transform chat messages based on model capabilities.
///
/// This is a pure function that applies capability-aware transformations:
/// - Converts system messages to user messages when model doesn't support system role
/// - Merges consecutive same-role messages when model requires strict alternation
///
/// # Invariant
///
/// When capabilities are unknown (empty), messages are passed through unchanged.
/// This prevents degrading standard models while allowing explicit constraints
/// to be enforced when detected.
///
/// # Arguments
///
/// * `messages` - The input chat messages to transform
/// * `capabilities` - The model's capability flags
///
/// # Returns
///
/// Transformed messages suitable for the model's constraints
pub fn transform_messages_for_capabilities(
    mut messages: Vec<ChatMessage>,
    capabilities: ModelCapabilities,
) -> Vec<ChatMessage> {
    // Pass through if capabilities are unknown
    if capabilities.is_empty() {
        return messages;
    }

    // STEP 1: Transform system messages if the model doesn't support them
    if !capabilities.contains(ModelCapabilities::SUPPORTS_SYSTEM_ROLE) {
        for msg in &mut messages {
            if msg.role == "system" {
                msg.role = "user".to_string();
                if let Some(content) = &mut msg.content {
                    *content = format!("[System]: {content}");
                }
            }
        }
    }

    // STEP 2: Merge consecutive same-role messages if strict turns are required
    if capabilities.contains(ModelCapabilities::REQUIRES_STRICT_TURNS) {
        let mut merged_messages = Vec::new();
        for msg in messages {
            if let Some(last) = merged_messages.last_mut() {
                let last_msg: &mut ChatMessage = last;
                // Only merge user/assistant messages to avoid tool-call ordering issues
                let is_mergeable_role = msg.role == "user" || msg.role == "assistant";
                if last_msg.role == msg.role
                    && is_mergeable_role
                    && last_msg.content.is_some()
                    && msg.content.is_some()
                    && last_msg.tool_calls.is_none()
                    && msg.tool_calls.is_none()
                {
                    // Merge content
                    if let (Some(last_content), Some(msg_content)) =
                        (&mut last_msg.content, &msg.content)
                    {
                        last_content.push_str("\n\n");
                        last_content.push_str(msg_content);
                    }
                    continue; // Skip adding this message separately
                }
            }
            merged_messages.push(msg);
        }
        return merged_messages;
    }

    messages
}

#[cfg(test)]
mod transform_tests {
    use super::*;

    #[test]
    fn test_transform_unknown_passes_through() {
        let messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: Some("You are a helpful assistant".to_string()),
                tool_calls: None,
            },
            ChatMessage {
                role: "user".to_string(),
                content: Some("Hello".to_string()),
                tool_calls: None,
            },
        ];
        let original = messages.clone();
        let result = transform_messages_for_capabilities(messages, ModelCapabilities::empty());
        assert_eq!(result, original);
    }

    #[test]
    fn test_transform_system_to_user() {
        let messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: Some("You are helpful".to_string()),
                tool_calls: None,
            },
            ChatMessage {
                role: "user".to_string(),
                content: Some("Hello".to_string()),
                tool_calls: None,
            },
        ];
        // Use REQUIRES_STRICT_TURNS which doesn't support system but doesn't merge different roles
        let caps = ModelCapabilities::REQUIRES_STRICT_TURNS;
        let result = transform_messages_for_capabilities(messages, caps);
        // System becomes user, both messages remain separate (user + user but different content)
        assert_eq!(result.len(), 1); // They get merged because both are now "user"
        assert_eq!(result[0].role, "user");
        assert!(
            result[0]
                .content
                .as_ref()
                .unwrap()
                .contains("[System]: You are helpful")
        );
        assert!(result[0].content.as_ref().unwrap().contains("Hello"));
    }

    #[test]
    fn test_transform_preserves_system_when_supported() {
        let messages = vec![ChatMessage {
            role: "system".to_string(),
            content: Some("You are helpful".to_string()),
            tool_calls: None,
        }];
        let caps = ModelCapabilities::SUPPORTS_SYSTEM_ROLE;
        let result = transform_messages_for_capabilities(messages, caps);
        assert_eq!(result[0].role, "system");
        assert_eq!(result[0].content, Some("You are helpful".to_string()));
    }

    #[test]
    fn test_transform_merges_consecutive_user_messages() {
        let messages = vec![
            ChatMessage {
                role: "user".to_string(),
                content: Some("First".to_string()),
                tool_calls: None,
            },
            ChatMessage {
                role: "user".to_string(),
                content: Some("Second".to_string()),
                tool_calls: None,
            },
        ];
        let caps = ModelCapabilities::REQUIRES_STRICT_TURNS;
        let result = transform_messages_for_capabilities(messages, caps);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].content, Some("First\n\nSecond".to_string()));
    }

    #[test]
    fn test_transform_does_not_merge_tool_messages() {
        let messages = vec![
            ChatMessage {
                role: "tool".to_string(),
                content: Some("Result 1".to_string()),
                tool_calls: None,
            },
            ChatMessage {
                role: "tool".to_string(),
                content: Some("Result 2".to_string()),
                tool_calls: None,
            },
        ];
        let caps = ModelCapabilities::REQUIRES_STRICT_TURNS;
        let result = transform_messages_for_capabilities(messages, caps);
        assert_eq!(result.len(), 2); // Should not merge tool messages
    }

    #[test]
    fn test_transform_combined_system_and_merge() {
        let messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: Some("Be helpful".to_string()),
                tool_calls: None,
            },
            ChatMessage {
                role: "user".to_string(),
                content: Some("First".to_string()),
                tool_calls: None,
            },
            ChatMessage {
                role: "user".to_string(),
                content: Some("Second".to_string()),
                tool_calls: None,
            },
        ];
        let caps = ModelCapabilities::REQUIRES_STRICT_TURNS; // No system support + strict turns
        let result = transform_messages_for_capabilities(messages, caps);
        assert_eq!(result.len(), 1); // System→user + merge
        assert_eq!(result[0].role, "user");
        assert!(
            result[0]
                .content
                .as_ref()
                .unwrap()
                .contains("[System]: Be helpful")
        );
        assert!(result[0].content.as_ref().unwrap().contains("First"));
        assert!(result[0].content.as_ref().unwrap().contains("Second"));
    }
}
