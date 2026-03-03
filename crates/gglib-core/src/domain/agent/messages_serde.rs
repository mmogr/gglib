//! Custom [`Serialize`] / [`Deserialize`] implementations for agent message types.
//!
//! Extracted from [`super::messages`] so the domain structs remain free of
//! serialisation noise.  The impls are automatically linked via the orphan
//! rules — `AssistantContent` is defined in the same crate.

use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize};

use super::messages::AssistantContent;
use super::tool_types::ToolCall;

// =============================================================================
// AssistantContent — Serialize
// =============================================================================

impl Serialize for AssistantContent {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let has_text = self.text.is_some();
        let has_calls = !self.tool_calls.is_empty();
        let count = usize::from(has_text) + usize::from(has_calls);
        let mut m = serializer.serialize_map(Some(count))?;
        if let Some(text) = &self.text {
            m.serialize_entry("content", text)?;
        }
        if has_calls {
            m.serialize_entry("tool_calls", &self.tool_calls)?;
        }
        m.end()
    }
}

// =============================================================================
// AssistantContent — Deserialize
// =============================================================================

impl<'de> Deserialize<'de> for AssistantContent {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(AssistantContentVisitor)
    }
}

/// Map visitor that reconstructs [`AssistantContent`] from a flat JSON map.
///
/// Accepts `"content"` (optional `String`) and `"tool_calls"` (optional
/// `Vec<ToolCall>`); at least one must be present.  Unknown keys are silently
/// ignored so the format is forward-compatible.
struct AssistantContentVisitor;

impl<'de> serde::de::Visitor<'de> for AssistantContentVisitor {
    type Value = AssistantContent;

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("assistant message with `content` and/or `tool_calls`")
    }

    fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut content: Option<String> = None;
        let mut tool_calls: Option<Vec<ToolCall>> = None;
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "content" => content = map.next_value()?,
                "tool_calls" => tool_calls = map.next_value()?,
                _ => {
                    map.next_value::<serde::de::IgnoredAny>()?;
                }
            }
        }
        let tool_calls = tool_calls.unwrap_or_default();
        if content.is_none() && tool_calls.is_empty() {
            return Err(serde::de::Error::custom(
                "assistant message must have `content` or `tool_calls`",
            ));
        }
        Ok(AssistantContent {
            text: content,
            tool_calls,
        })
    }
}
