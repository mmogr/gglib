//! [`AgentMessage`] — A single message in the agent conversation.

use serde::{Deserialize, Serialize};

use super::tool_types::ToolCall;

/// Content carried by an [`AgentMessage::Assistant`] turn.
///
/// Enforces that an assistant message always has *at least* one of text or tool
/// calls, making the vacuous all-`None` state impossible to construct.
///
/// # Serde
///
/// Serializes/deserializes as a flat map so it can be `#[serde(flatten)]`-ed
/// directly into the parent [`AgentMessage`] object:
///
/// | Variant | JSON fields |
/// |---------|-------------|
/// | `Content(s)` | `"content": "…"` |
/// | `ToolCalls(tcs)` | `"tool_calls": […]` |
/// | `Both(s, tcs)` | `"content": "…", "tool_calls": […]` |
///
/// # Why not `#[serde(untagged)]`?
///
/// `#[serde(untagged)]` cannot enforce the “at least one field present”
/// invariant: a JSON object `{}` (missing both `content` and `tool_calls`)
/// would silently deserialise to an arbitrary variant rather than returning a
/// meaningful error.  The hand-rolled [`Deserialize`] impl rejects that case
/// with `"assistant message must have content or tool_calls"`.
/// The [`Serialize`] impl mirrors the structure exactly so round-trips are
/// lossless.
#[derive(Debug, Clone)]
pub enum AssistantContent {
    /// Text response only (no tool calls).
    Content(String),
    /// Tool calls only (model produced no text preamble).
    ToolCalls(Vec<ToolCall>),
    /// Text preamble followed by tool calls.
    Both(String, Vec<ToolCall>),
}

impl AssistantContent {
    /// Return the text content if present.
    pub const fn text(&self) -> Option<&str> {
        match self {
            Self::Content(s) | Self::Both(s, _) => Some(s.as_str()),
            Self::ToolCalls(_) => None,
        }
    }

    /// Return the tool calls if present.
    pub const fn tool_calls(&self) -> Option<&[ToolCall]> {
        match self {
            Self::ToolCalls(tcs) | Self::Both(_, tcs) => Some(tcs.as_slice()),
            Self::Content(_) => None,
        }
    }

    /// Consume `self` and return a new variant with `calls` as the tool-call
    /// list, preserving any existing text content.
    #[must_use]
    pub fn with_replaced_tool_calls(self, calls: Vec<ToolCall>) -> Self {
        match self {
            Self::Content(s) | Self::Both(s, _) => Self::Both(s, calls),
            Self::ToolCalls(_) => Self::ToolCalls(calls),
        }
    }
}

impl Serialize for AssistantContent {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        match self {
            Self::Content(text) => {
                let mut m = serializer.serialize_map(Some(1))?;
                m.serialize_entry("content", text)?;
                m.end()
            }
            Self::ToolCalls(tcs) => {
                let mut m = serializer.serialize_map(Some(1))?;
                m.serialize_entry("tool_calls", tcs)?;
                m.end()
            }
            Self::Both(text, tcs) => {
                let mut m = serializer.serialize_map(Some(2))?;
                m.serialize_entry("content", text)?;
                m.serialize_entry("tool_calls", tcs)?;
                m.end()
            }
        }
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
                match (content, tool_calls) {
                    (Some(s), None) => Ok(AssistantContent::Content(s)),
                    (None, Some(tcs)) => Ok(AssistantContent::ToolCalls(tcs)),
                    (Some(s), Some(tcs)) => Ok(AssistantContent::Both(s, tcs)),
                    (None, None) => Err(serde::de::Error::custom(
                        "assistant message must have `content` or `tool_calls`",
                    )),
                }
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
                content.text().map_or(0, |s| s.chars().count())
                    + content.tool_calls().map_or(0, |tcs| {
                        tcs.iter()
                            .map(|c| {
                                // Include `id` so the context-budget estimate
                                // matches what llama-server actually tokenises
                                // (a typical id like "call_abc123" is ~15 chars).
                                c.id.chars().count()
                                    + c.name.chars().count()
                                    + c.arguments.to_string().chars().count()
                            })
                            .sum()
                    })
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
            content: AssistantContent::Content("hi".into()),
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
            content: AssistantContent::ToolCalls(vec![ToolCall {
                id: "c1".into(),
                name: "search".into(),
                arguments: json!({}),
            }]),
        };
        let json_val = serde_json::to_value(&msg).unwrap();
        assert_eq!(json_val["role"], "assistant");
        assert!(json_val.get("content").is_none());
        assert!(json_val["tool_calls"].is_array());
    }

    /// Verify that the custom Serde deserializer reconstructs
    /// [`AssistantContent::Both`] correctly on a round-trip.
    ///
    /// Some LLMs (e.g. models with parallel function calling) emit a non-empty
    /// `content` string alongside `tool_calls` in the same assistant message.
    /// The round-trip must preserve both arms exactly.
    #[test]
    fn assistant_both_round_trips() {
        use serde_json::json;

        let original = AgentMessage::Assistant {
            content: AssistantContent::Both(
                "thinking out loud".into(),
                vec![
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
            ),
        };

        // Serialise → deserialise.
        let json_val = serde_json::to_value(&original).unwrap();
        assert_eq!(json_val["role"], "assistant");
        assert_eq!(
            json_val["content"], "thinking out loud",
            "content arm must be present"
        );
        assert_eq!(
            json_val["tool_calls"].as_array().unwrap().len(),
            2,
            "tool_calls arm must be present with 2 entries"
        );

        // Round-trip: deserialise back from the serialised value.
        let reconstructed: AgentMessage = serde_json::from_value(json_val).unwrap();
        if let AgentMessage::Assistant { content } = reconstructed {
            if let AssistantContent::Both(text, calls) = content {
                assert_eq!(text, "thinking out loud");
                assert_eq!(calls.len(), 2);
                assert_eq!(calls[0].id, "c1");
                assert_eq!(calls[1].name, "read_file");
            } else {
                panic!("expected AssistantContent::Both, got something else");
            }
        } else {
            panic!("expected AgentMessage::Assistant");
        }
    }

    #[test]
    fn with_replaced_tool_calls_content_becomes_both() {
        use serde_json::json;
        let original = AssistantContent::Content("hello".into());
        let calls = vec![ToolCall {
            id: "c1".into(),
            name: "search".into(),
            arguments: json!({}),
        }];
        let result = original.with_replaced_tool_calls(calls.clone());
        match result {
            AssistantContent::Both(text, tcs) => {
                assert_eq!(text, "hello");
                assert_eq!(tcs.len(), 1);
                assert_eq!(tcs[0].id, "c1");
            }
            other => panic!("expected Both, got {other:?}"),
        }
    }

    #[test]
    fn with_replaced_tool_calls_both_replaces_calls() {
        use serde_json::json;
        let original = AssistantContent::Both(
            "thinking".into(),
            vec![ToolCall {
                id: "old".into(),
                name: "old_tool".into(),
                arguments: json!({}),
            }],
        );
        let new_calls = vec![ToolCall {
            id: "new".into(),
            name: "new_tool".into(),
            arguments: json!({"key": "val"}),
        }];
        let result = original.with_replaced_tool_calls(new_calls);
        match result {
            AssistantContent::Both(text, tcs) => {
                assert_eq!(text, "thinking");
                assert_eq!(tcs.len(), 1);
                assert_eq!(tcs[0].name, "new_tool");
            }
            other => panic!("expected Both, got {other:?}"),
        }
    }

    #[test]
    fn with_replaced_tool_calls_tool_calls_only_stays_tool_calls() {
        use serde_json::json;
        let original = AssistantContent::ToolCalls(vec![ToolCall {
            id: "old".into(),
            name: "old".into(),
            arguments: json!({}),
        }]);
        let new_calls = vec![ToolCall {
            id: "new".into(),
            name: "new".into(),
            arguments: json!({}),
        }];
        let result = original.with_replaced_tool_calls(new_calls);
        match result {
            AssistantContent::ToolCalls(tcs) => {
                assert_eq!(tcs.len(), 1);
                assert_eq!(tcs[0].id, "new");
            }
            other => panic!("expected ToolCalls, got {other:?}"),
        }
    }
}
