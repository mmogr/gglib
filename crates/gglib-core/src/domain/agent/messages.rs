//! [`AgentMessage`] — A single message in the agent conversation.

use serde::{Deserialize, Serialize};

use super::tool_types::ToolCall;

/// Content carried by an [`AgentMessage::Assistant`] turn.
///
/// A flat struct with optional `text` and a (possibly empty) `tool_calls` vec.
/// At the wire level, at least one of the two fields must be present — the
/// hand-rolled [`Deserialize`] impl enforces this.
///
/// # Serde
///
/// Serializes/deserializes as a flat map so it can be `#[serde(flatten)]`-ed
/// directly into the parent [`AgentMessage`] object:
///
/// | State | JSON fields |
/// |-------|-------------|
/// | text only | `"content": "..."` |
/// | tool calls only | `"tool_calls": [...]` |
/// | both | `"content": "...", "tool_calls": [...]` |
#[derive(Debug, Clone)]
pub struct AssistantContent {
    /// Optional text content from the model.  `None` when the model produced
    /// only tool calls with no text preamble.
    pub text: Option<String>,
    /// Tool calls requested by the model.  Empty when the model produced a
    /// text-only response (final answer).
    pub tool_calls: Vec<ToolCall>,
}

impl AssistantContent {
    /// Consume `self` and return a new value with `calls` as the tool-call
    /// list, preserving any existing text content.
    #[must_use]
    pub fn with_replaced_tool_calls(self, calls: Vec<ToolCall>) -> Self {
        Self {
            tool_calls: calls,
            ..self
        }
    }
}

impl Serialize for AssistantContent {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
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

impl<'de> Deserialize<'de> for AssistantContent {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = AssistantContent;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("assistant message with `content` and/or `tool_calls`")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Self::Value, A::Error> {
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
        deserializer.deserialize_map(V)
    }
}

/// A single message in the agent conversation.
///
/// The closed enum prevents invalid states that a flat struct with `role: String`
/// would allow (e.g. a `User` message carrying `tool_calls`, or a `Tool` message
/// without a `tool_call_id`).
///
/// # Wire format
///
/// `#[serde(tag = "role", rename_all = "lowercase")]` produces JSON identical to
/// the TypeScript `ChatMessage` interface in the frontend:
///
/// ```json
/// { "role": "user", "content": "What files are in the project?" }
/// { "role": "assistant", "content": null, "tool_calls": [...] }
/// { "role": "tool", "tool_call_id": "call_abc", "content": "src/\nlib/" }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum AgentMessage {
    /// A system-level instruction that sets the model's persona and constraints.
    System {
        /// Instruction text.
        content: String,
    },

    /// A message from the human user.
    User {
        /// Message text.
        content: String,
    },

    /// A response from the assistant model.
    ///
    /// `content` always carries either text, tool calls, or both — the
    /// vacuous all-`None` state of the previous `Option<String>` +
    /// `Option<Vec<ToolCall>>` representation is impossible to construct.
    Assistant {
        /// Content of the assistant turn.
        #[serde(flatten)]
        content: AssistantContent,
    },

    /// The result of a tool call, to be sent back to the model.
    Tool {
        /// Must match the [`ToolCall::id`] from the preceding `Assistant` message.
        tool_call_id: String,

        /// Serialised output of the tool (or error description if it failed).
        content: String,
    },
}

impl AgentMessage {
    /// Estimate the Unicode scalar-value count of this message.
    ///
    /// Uses [`str::chars().count()`] rather than [`str::len`] (byte count) so
    /// that multi-byte characters are counted as one unit, matching how LLMs
    /// typically measure context length.
    ///
    /// # Performance
    ///
    /// This is an **O(n)** scan — it iterates over every Unicode scalar value
    /// in every `str` field of the message. Avoid calling it inside tight or
    /// nested loops. For repeated measurements over the same message set,
    /// accumulate the total once and update it incrementally (the agent loop
    /// does exactly this via its `running_chars` counter).
    pub fn char_count(&self) -> usize {
        match self {
            Self::System { content } | Self::User { content } => content.chars().count(),
            Self::Assistant { content } => {
                content.text.as_ref().map_or(0, |s| s.chars().count())
                    + content
                        .tool_calls
                        .iter()
                        .map(|c| {
                            // Include `id` so the context-budget estimate
                            // matches what llama-server actually tokenises
                            // (a typical id like "call_abc123" is ~15 chars).
                            c.id.chars().count()
                                + c.name.chars().count()
                                + c.arguments.to_string().chars().count()
                        })
                        .sum::<usize>()
            }
            Self::Tool {
                tool_call_id,
                content,
            } => tool_call_id.chars().count() + content.chars().count(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_tag_matches_wire_format() {
        let msg = AgentMessage::Tool {
            tool_call_id: "call_1".into(),
            content: "ok".into(),
        };
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["role"], "tool");
        assert_eq!(json["tool_call_id"], "call_1");
    }

    #[test]
    fn assistant_content_only_omits_tool_calls() {
        let msg = AgentMessage::Assistant {
            content: AssistantContent {
                text: Some("hi".into()),
                tool_calls: vec![],
            },
        };
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["role"], "assistant");
        assert_eq!(json["content"], "hi");
        assert!(json.get("tool_calls").is_none());
    }

    #[test]
    fn assistant_tool_calls_only_omits_content() {
        use serde_json::json;
        let msg = AgentMessage::Assistant {
            content: AssistantContent {
                text: None,
                tool_calls: vec![ToolCall {
                    id: "c1".into(),
                    name: "search".into(),
                    arguments: json!({}),
                }],
            },
        };
        let json_val = serde_json::to_value(&msg).unwrap();
        assert_eq!(json_val["role"], "assistant");
        assert!(json_val.get("content").is_none());
        assert!(json_val["tool_calls"].is_array());
    }

    /// Verify that the custom Serde deserializer reconstructs
    /// [`AssistantContent`] correctly on a round-trip when both text and
    /// tool calls are present.
    ///
    /// Some LLMs (e.g. models with parallel function calling) emit a non-empty
    /// `content` string alongside `tool_calls` in the same assistant message.
    /// The round-trip must preserve both fields exactly.
    #[test]
    fn assistant_both_round_trips() {
        use serde_json::json;

        let original = AgentMessage::Assistant {
            content: AssistantContent {
                text: Some("thinking out loud".into()),
                tool_calls: vec![
                    ToolCall {
                        id: "c1".into(),
                        name: "web_search".into(),
                        arguments: json!({ "query": "rust async" }),
                    },
                    ToolCall {
                        id: "c2".into(),
                        name: "read_file".into(),
                        arguments: json!({ "path": "/tmp/x" }),
                    },
                ],
            },
        };

        // Serialise -> deserialise.
        let json_val = serde_json::to_value(&original).unwrap();
        assert_eq!(json_val["role"], "assistant");
        assert_eq!(
            json_val["content"], "thinking out loud",
            "content must be present"
        );
        assert_eq!(
            json_val["tool_calls"].as_array().unwrap().len(),
            2,
            "tool_calls must be present with 2 entries"
        );

        // Round-trip: deserialise back from the serialised value.
        let reconstructed: AgentMessage = serde_json::from_value(json_val).unwrap();
        if let AgentMessage::Assistant { content } = reconstructed {
            assert_eq!(content.text.as_deref(), Some("thinking out loud"));
            assert_eq!(content.tool_calls.len(), 2);
            assert_eq!(content.tool_calls[0].id, "c1");
            assert_eq!(content.tool_calls[1].name, "read_file");
        } else {
            panic!("expected AgentMessage::Assistant");
        }
    }

    #[test]
    fn with_replaced_tool_calls_preserves_text() {
        use serde_json::json;
        let original = AssistantContent {
            text: Some("hello".into()),
            tool_calls: vec![],
        };
        let calls = vec![ToolCall {
            id: "c1".into(),
            name: "search".into(),
            arguments: json!({}),
        }];
        let result = original.with_replaced_tool_calls(calls);
        assert_eq!(result.text.as_deref(), Some("hello"));
        assert_eq!(result.tool_calls.len(), 1);
        assert_eq!(result.tool_calls[0].id, "c1");
    }

    #[test]
    fn with_replaced_tool_calls_replaces_existing() {
        use serde_json::json;
        let original = AssistantContent {
            text: Some("thinking".into()),
            tool_calls: vec![ToolCall {
                id: "old".into(),
                name: "old_tool".into(),
                arguments: json!({}),
            }],
        };
        let new_calls = vec![ToolCall {
            id: "new".into(),
            name: "new_tool".into(),
            arguments: json!({"key": "val"}),
        }];
        let result = original.with_replaced_tool_calls(new_calls);
        assert_eq!(result.text.as_deref(), Some("thinking"));
        assert_eq!(result.tool_calls.len(), 1);
        assert_eq!(result.tool_calls[0].name, "new_tool");
    }

    #[test]
    fn with_replaced_tool_calls_no_text() {
        use serde_json::json;
        let original = AssistantContent {
            text: None,
            tool_calls: vec![ToolCall {
                id: "old".into(),
                name: "old".into(),
                arguments: json!({}),
            }],
        };
        let new_calls = vec![ToolCall {
            id: "new".into(),
            name: "new".into(),
            arguments: json!({}),
        }];
        let result = original.with_replaced_tool_calls(new_calls);
        assert!(result.text.is_none());
        assert_eq!(result.tool_calls.len(), 1);
        assert_eq!(result.tool_calls[0].id, "new");
    }
}
