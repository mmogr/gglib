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

/// Infer model capabilities from chat template Jinja source and model name.
///
/// Uses string heuristics to detect template constraints. Returns safe
/// defaults if template is missing or unparseable.
///
/// # Detection Strategy
///
/// Two-layer approach:
/// - **Layer 1 (Metadata)**: Check chat template for reliable signals (preferred)
/// - **Layer 2 (Name Heuristics)**: Use model name patterns as fallback when metadata is missing
///
/// # Capabilities Detected
///
/// - System role: Looks for explicit rejection messages in template
/// - Strict turns: Looks for alternation enforcement logic
/// - Tool calling: Checks for `<tool_call>`, `if tools`, `function_call` patterns (metadata);
///   falls back to model name patterns like "hermes", "functionary" (heuristic)
/// - Reasoning: Checks for `<think>`, `<reasoning>`, `enable_thinking` (metadata);
///   falls back to model name patterns like "deepseek-r1", "qwq", "o1" (heuristic)
///
/// # Fallback Behavior
///
/// Missing or unparseable templates default to empty capabilities (unknown state).
pub fn infer_from_chat_template(
    template: Option<&str>,
    model_name: Option<&str>,
) -> ModelCapabilities {
    let mut caps = ModelCapabilities::empty();

    // ─────────────────────────────────────────────────────────────────────────────
    // Layer 1: Metadata-based detection (chat template analysis)
    // ─────────────────────────────────────────────────────────────────────────────

    let mut tool_detected_from_metadata = false;
    let mut reasoning_detected_from_metadata = false;

    if let Some(template) = template {
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

        // Detect tool calling support from template
        let has_tool_patterns = template.contains("<tool_call>")
            || template.contains("<|python_tag|>")
            || template.contains("if tools")
            || template.contains("tools is defined")
            || template.contains("tool_calls")
            || template.contains("function_call");

        if has_tool_patterns {
            caps |= ModelCapabilities::SUPPORTS_TOOL_CALLS;
            tool_detected_from_metadata = true;
        }

        // Detect reasoning/thinking support from template
        let has_reasoning_patterns = template.contains("<think>")
            || template.contains("</think>")
            || template.contains("<reasoning>")
            || template.contains("</reasoning>")
            || template.contains("enable_thinking")
            || template.contains("thinking_forced_open")
            || template.contains("reasoning_content");

        if has_reasoning_patterns {
            caps |= ModelCapabilities::SUPPORTS_REASONING;
            reasoning_detected_from_metadata = true;
        }
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // Layer 2: Name-based heuristic fallback (when metadata is inconclusive)
    // ─────────────────────────────────────────────────────────────────────────────
    //
    // Only use name patterns when chat template didn't provide clear evidence.
    // This is less reliable but helps with models that have incomplete metadata.

    if let Some(name) = model_name {
        let name_lower = name.to_lowercase();

        // Heuristic: Tool calling support based on model name
        if !tool_detected_from_metadata {
            let has_tool_name = name_lower.contains("hermes")
                || name_lower.contains("functionary")
                || name_lower.contains("firefunction")
                || name_lower.contains("gorilla");

            if has_tool_name {
                caps |= ModelCapabilities::SUPPORTS_TOOL_CALLS;
            }
        }

        // Heuristic: Reasoning support based on model name
        if !reasoning_detected_from_metadata {
            let has_reasoning_name = name_lower.contains("deepseek-r1")
                || name_lower.contains("qwq")
                || name_lower.contains("-r1-")
                || name_lower.contains("o1");

            if has_reasoning_name {
                caps |= ModelCapabilities::SUPPORTS_REASONING;
            }
        }
    }

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
        assert!(!caps.supports_tool_calls());
        assert!(!caps.supports_reasoning());
    }

    #[test]
    fn test_infer_openai_style() {
        let template = r"
            {% for message in messages %}
                {{ message.role }}: {{ message.content }}
            {% endfor %}
        ";
        let caps = infer_from_chat_template(Some(template), None);
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
        let caps = infer_from_chat_template(Some(template), None);
        assert!(!caps.supports_system_role());
        assert!(caps.requires_strict_turns());
    }

    #[test]
    fn test_infer_missing_template() {
        let caps = infer_from_chat_template(None, None);
        // Missing template means unknown capabilities - no assumptions made
        assert!(caps.is_empty());
        assert!(!caps.supports_system_role());
    }

    #[test]
    fn test_tool_calling_from_template() {
        let template = r"
            {% if tools %}
                <tool_call>{{ message.tool_calls }}</tool_call>
            {% endif %}
        ";
        let caps = infer_from_chat_template(Some(template), None);
        assert!(caps.supports_tool_calls());
    }

    #[test]
    fn test_reasoning_from_template() {
        let template = r"
            {% if enable_thinking %}
                <think>{{ message.thinking }}</think>
            {% endif %}
        ";
        let caps = infer_from_chat_template(Some(template), None);
        assert!(caps.supports_reasoning());
    }

    #[test]
    fn test_tool_calling_name_fallback() {
        // No template, but model name suggests tool support
        let caps = infer_from_chat_template(None, Some("hermes-2-pro-7b"));
        assert!(caps.supports_tool_calls());
    }

    #[test]
    fn test_reasoning_name_fallback() {
        // No template, but model name suggests reasoning support
        let caps = infer_from_chat_template(None, Some("deepseek-r1-lite"));
        assert!(caps.supports_reasoning());
    }

    #[test]
    fn test_metadata_plus_name_fallback() {
        // Template present but has no tool markers - should still use name fallback
        let template = "simple template with no tool markers";
        let caps = infer_from_chat_template(Some(template), Some("hermes-model"));
        // Name fallback should kick in because metadata didn't detect tools
        assert!(caps.supports_tool_calls());
    }

    #[test]
    fn test_metadata_detected_skips_name_fallback() {
        // When metadata detects capability, name pattern is ignored
        let template = "<tool_call>detected</tool_call>";
        let caps = infer_from_chat_template(Some(template), Some("not-a-tool-model"));
        // Metadata detected it, so tool support is enabled regardless of name
        assert!(caps.supports_tool_calls());
    }

    #[test]
    fn test_combined_detections() {
        let template = r"
            {% if tools %}<tool_call>{{ tool }}</tool_call>{% endif %}
            <think>{{ reasoning }}</think>
        ";
        let caps = infer_from_chat_template(Some(template), None);
        assert!(caps.supports_tool_calls());
        assert!(caps.supports_reasoning());
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

/// Merge consecutive system messages into a single message.
///
/// This is universally safe because:
/// - No model template requires multiple system messages
/// - Merging preserves all content with clear separation
/// - It prevents errors in strict-turn templates (e.g., gemma3/medgemma)
///
/// # Arguments
///
/// * `messages` - The input chat messages
///
/// # Returns
///
/// Messages with consecutive system messages merged
fn merge_consecutive_system_messages(messages: Vec<ChatMessage>) -> Vec<ChatMessage> {
    if messages.is_empty() {
        return messages;
    }

    let mut result: Vec<ChatMessage> = Vec::with_capacity(messages.len());

    for msg in messages {
        if let Some(last) = result.last_mut() {
            if last.role == "system" && msg.role == "system" {
                // Merge: append content with separator
                let last_content = last.content.take().unwrap_or_default();
                let new_content = msg.content.unwrap_or_default();

                last.content = Some(if last_content.is_empty() {
                    new_content
                } else if new_content.is_empty() {
                    last_content
                } else {
                    format!("{last_content}\n\n{new_content}")
                });

                continue; // Don't push, we merged into last
            }
        }
        result.push(msg);
    }

    result
}

/// Transform chat messages based on model capabilities.
///
/// This is a pure function that applies capability-aware transformations:
/// - Merges consecutive system messages (always, for all models)
/// - Converts system messages to user messages when model doesn't support system role
/// - Merges consecutive same-role messages when model requires strict alternation
///
/// # Invariant
///
/// Consecutive system messages are ALWAYS merged, regardless of capabilities.
/// This prevents Jinja template errors in models with strict role alternation.
///
/// When capabilities are unknown (empty), only system message merging is applied.
/// This prevents degrading standard models while ensuring universal compatibility.
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
    // STEP 0 (ALWAYS): Merge consecutive system messages.
    // This is safe for ALL models and prevents Jinja template errors
    // in models with strict role alternation (e.g., gemma3/medgemma).
    // Must run BEFORE the capabilities check to protect unknown models.
    messages = merge_consecutive_system_messages(messages);

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
                // Only merge user/assistant messages (avoid merging tool/system messages)
                let is_mergeable_role = msg.role == "user" || msg.role == "assistant";

                // Merge if same role and mergeable (user or assistant)
                if last_msg.role == msg.role && is_mergeable_role {
                    // Merge content if both have content
                    match (&mut last_msg.content, &msg.content) {
                        (Some(last_content), Some(msg_content)) => {
                            // Both have content - merge with separator
                            last_content.push_str("\n\n");
                            last_content.push_str(msg_content);
                        }
                        (None, Some(msg_content)) => {
                            // Only new message has content - use it
                            last_msg.content = Some(msg_content.clone());
                        }
                        // If last has content and msg doesn't, keep last's content
                        // If neither have content, keep None
                        _ => {}
                    }

                    // Merge tool_calls if present
                    match (&mut last_msg.tool_calls, &msg.tool_calls) {
                        (Some(last_calls), Some(msg_calls)) => {
                            // Both have tool_calls - concatenate arrays
                            if let (Some(last_arr), Some(msg_arr)) =
                                (last_calls.as_array_mut(), msg_calls.as_array())
                            {
                                last_arr.extend_from_slice(msg_arr);
                            }
                        }
                        (None, Some(msg_calls)) => {
                            // Only new message has tool_calls - use it
                            last_msg.tool_calls = Some(msg_calls.clone());
                        }
                        // If last has tool_calls and msg doesn't, keep last's tool_calls
                        // If neither have tool_calls, keep None
                        _ => {}
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
    fn test_transform_unknown_passes_through_non_system() {
        // Non-system messages pass through unchanged with empty capabilities
        let messages = vec![
            ChatMessage {
                role: "user".to_string(),
                content: Some("Hello".to_string()),
                tool_calls: None,
            },
            ChatMessage {
                role: "assistant".to_string(),
                content: Some("Hi there".to_string()),
                tool_calls: None,
            },
        ];
        let original = messages.clone();
        let result = transform_messages_for_capabilities(messages, ModelCapabilities::empty());
        assert_eq!(result, original);
    }

    #[test]
    fn test_merges_consecutive_system_messages_always() {
        // Even with empty capabilities, consecutive system messages should merge
        let messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: Some("You are a helpful assistant.".to_string()),
                tool_calls: None,
            },
            ChatMessage {
                role: "system".to_string(),
                content: Some("WORKING_MEMORY:\n- task1 (ok): done".to_string()),
                tool_calls: None,
            },
            ChatMessage {
                role: "user".to_string(),
                content: Some("Hello".to_string()),
                tool_calls: None,
            },
        ];
        let result = transform_messages_for_capabilities(messages, ModelCapabilities::empty());

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].role, "system");
        assert_eq!(
            result[0].content.as_deref(),
            Some("You are a helpful assistant.\n\nWORKING_MEMORY:\n- task1 (ok): done")
        );
        assert_eq!(result[1].role, "user");
    }

    #[test]
    fn test_merges_three_consecutive_system_messages() {
        let messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: Some("First.".to_string()),
                tool_calls: None,
            },
            ChatMessage {
                role: "system".to_string(),
                content: Some("Second.".to_string()),
                tool_calls: None,
            },
            ChatMessage {
                role: "system".to_string(),
                content: Some("Third.".to_string()),
                tool_calls: None,
            },
        ];
        let result = transform_messages_for_capabilities(messages, ModelCapabilities::empty());

        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].content.as_deref(),
            Some("First.\n\nSecond.\n\nThird.")
        );
    }

    #[test]
    fn test_handles_empty_system_content() {
        let messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: Some(String::new()),
                tool_calls: None,
            },
            ChatMessage {
                role: "system".to_string(),
                content: Some("Actual content".to_string()),
                tool_calls: None,
            },
        ];
        let result = transform_messages_for_capabilities(messages, ModelCapabilities::empty());

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].content.as_deref(), Some("Actual content"));
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

    #[test]
    fn test_merge_consecutive_assistant_with_tool_calls() {
        // This is the main bug fix: consecutive assistant messages with tool_calls
        // should be merged for models requiring strict turns
        let tool_call_1 = serde_json::json!([
            {
                "id": "call_1",
                "type": "function",
                "function": {
                    "name": "get_weather",
                    "arguments": "{\"location\":\"Paris\"}"
                }
            }
        ]);
        let tool_call_2 = serde_json::json!([
            {
                "id": "call_2",
                "type": "function",
                "function": {
                    "name": "get_time",
                    "arguments": "{\"timezone\":\"UTC\"}"
                }
            }
        ]);

        let messages = vec![
            ChatMessage {
                role: "user".to_string(),
                content: Some("What's the weather?".to_string()),
                tool_calls: None,
            },
            ChatMessage {
                role: "assistant".to_string(),
                content: Some("Let me check...".to_string()),
                tool_calls: Some(tool_call_1),
            },
            ChatMessage {
                role: "assistant".to_string(),
                content: Some("And the time...".to_string()),
                tool_calls: Some(tool_call_2),
            },
        ];

        let caps = ModelCapabilities::REQUIRES_STRICT_TURNS;
        let result = transform_messages_for_capabilities(messages, caps);

        // Should merge into 2 messages: user + merged assistant
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].role, "user");
        assert_eq!(result[1].role, "assistant");

        // Content should be merged
        assert_eq!(
            result[1].content,
            Some("Let me check...\n\nAnd the time...".to_string())
        );

        // Tool calls should be concatenated
        let merged_tool_calls = result[1].tool_calls.as_ref().unwrap();
        let tool_calls_array = merged_tool_calls.as_array().unwrap();
        assert_eq!(tool_calls_array.len(), 2);
        assert_eq!(tool_calls_array[0]["id"], "call_1");
        assert_eq!(tool_calls_array[1]["id"], "call_2");
    }

    #[test]
    fn test_merge_assistant_messages_only_first_has_content() {
        // First message has content, second has only tool_calls
        let tool_call = serde_json::json!([
            {
                "id": "call_1",
                "type": "function",
                "function": {
                    "name": "get_weather",
                    "arguments": "{}"
                }
            }
        ]);

        let messages = vec![
            ChatMessage {
                role: "assistant".to_string(),
                content: Some("Let me check...".to_string()),
                tool_calls: None,
            },
            ChatMessage {
                role: "assistant".to_string(),
                content: None,
                tool_calls: Some(tool_call),
            },
        ];

        let caps = ModelCapabilities::REQUIRES_STRICT_TURNS;
        let result = transform_messages_for_capabilities(messages, caps);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].content, Some("Let me check...".to_string()));
        assert!(result[0].tool_calls.is_some());
    }

    #[test]
    fn test_merge_assistant_messages_only_second_has_content() {
        // First message has only tool_calls, second has content
        let tool_call = serde_json::json!([
            {
                "id": "call_1",
                "type": "function",
                "function": {
                    "name": "get_weather",
                    "arguments": "{}"
                }
            }
        ]);

        let messages = vec![
            ChatMessage {
                role: "assistant".to_string(),
                content: None,
                tool_calls: Some(tool_call),
            },
            ChatMessage {
                role: "assistant".to_string(),
                content: Some("Result received".to_string()),
                tool_calls: None,
            },
        ];

        let caps = ModelCapabilities::REQUIRES_STRICT_TURNS;
        let result = transform_messages_for_capabilities(messages, caps);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].content, Some("Result received".to_string()));
        assert!(result[0].tool_calls.is_some());
    }

    #[test]
    fn test_merge_assistant_messages_neither_has_content() {
        // Both messages have only tool_calls, no content
        let tool_call_1 = serde_json::json!([
            {
                "id": "call_1",
                "type": "function",
                "function": {"name": "tool1", "arguments": "{}"}
            }
        ]);
        let tool_call_2 = serde_json::json!([
            {
                "id": "call_2",
                "type": "function",
                "function": {"name": "tool2", "arguments": "{}"}
            }
        ]);

        let messages = vec![
            ChatMessage {
                role: "assistant".to_string(),
                content: None,
                tool_calls: Some(tool_call_1),
            },
            ChatMessage {
                role: "assistant".to_string(),
                content: None,
                tool_calls: Some(tool_call_2),
            },
        ];

        let caps = ModelCapabilities::REQUIRES_STRICT_TURNS;
        let result = transform_messages_for_capabilities(messages, caps);

        assert_eq!(result.len(), 1);
        assert!(result[0].content.is_none());

        let merged_tool_calls = result[0].tool_calls.as_ref().unwrap();
        let tool_calls_array = merged_tool_calls.as_array().unwrap();
        assert_eq!(tool_calls_array.len(), 2);
    }

    #[test]
    fn test_no_merge_without_strict_turns_capability() {
        // Even with consecutive assistant messages, don't merge if capability not set
        let messages = vec![
            ChatMessage {
                role: "assistant".to_string(),
                content: Some("First".to_string()),
                tool_calls: None,
            },
            ChatMessage {
                role: "assistant".to_string(),
                content: Some("Second".to_string()),
                tool_calls: None,
            },
        ];

        let caps = ModelCapabilities::empty();
        let result = transform_messages_for_capabilities(messages, caps);

        // Should NOT merge without REQUIRES_STRICT_TURNS capability
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_merge_preserves_different_role_boundaries() {
        // Don't merge across different roles
        let tool_call = serde_json::json!([
            {
                "id": "call_1",
                "type": "function",
                "function": {"name": "tool1", "arguments": "{}"}
            }
        ]);

        let messages = vec![
            ChatMessage {
                role: "user".to_string(),
                content: Some("Question".to_string()),
                tool_calls: None,
            },
            ChatMessage {
                role: "assistant".to_string(),
                content: Some("Answer".to_string()),
                tool_calls: Some(tool_call),
            },
            ChatMessage {
                role: "user".to_string(),
                content: Some("Follow-up".to_string()),
                tool_calls: None,
            },
        ];

        let caps = ModelCapabilities::REQUIRES_STRICT_TURNS;
        let result = transform_messages_for_capabilities(messages, caps);

        // Should remain 3 separate messages
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].role, "user");
        assert_eq!(result[1].role, "assistant");
        assert_eq!(result[2].role, "user");
    }
}
