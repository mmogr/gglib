//! [`AgentMessage`] on the wire, and what it is charged against the context.

use super::*;

fn image(bytes: &[u8]) -> AttachmentId {
    AttachmentId::of(bytes)
}

/// A user message with no image is, byte for byte, what it was before a
/// message could carry one: no `images` key.
#[test]
fn a_text_only_user_message_serialises_with_no_images_key() {
    let msg = AgentMessage::User {
        content: "What files are in the project?".into(),
        images: Vec::new(),
    };
    assert_eq!(
        serde_json::to_string(&msg).unwrap(),
        r#"{"role":"user","content":"What files are in the project?"}"#
    );
    assert_eq!(
        serde_json::to_string(&AgentMessage::user("hi")).unwrap(),
        r#"{"role":"user","content":"hi"}"#
    );
}

#[test]
fn a_user_message_with_no_images_key_deserialises_to_none() {
    let msg: AgentMessage = serde_json::from_str(r#"{"role":"user","content":"hi"}"#).unwrap();
    let AgentMessage::User { content, images } = msg else {
        panic!("expected AgentMessage::User");
    };
    assert_eq!(content, "hi");
    assert!(images.is_empty());
}

#[test]
fn a_user_message_carries_its_images_by_id_in_order() {
    let (a, b) = (image(b"a"), image(b"b"));
    let msg = AgentMessage::User {
        content: "look".into(),
        images: vec![b.clone(), a.clone()],
    };
    let json = serde_json::to_value(&msg).unwrap();
    assert_eq!(
        json,
        serde_json::json!({"role": "user", "content": "look", "images": [b.as_str(), a.as_str()]})
    );
    let AgentMessage::User { images, .. } = serde_json::from_value(json).unwrap() else {
        panic!("expected AgentMessage::User");
    };
    assert_eq!(images, [b, a]);
}

#[test]
fn a_user_message_naming_something_that_is_no_id_does_not_deserialise() {
    let json = r#"{"role":"user","content":"look","images":["data:image/png;base64,AAAA"]}"#;
    assert!(serde_json::from_str::<AgentMessage>(json).is_err());
}

/// Each image is charged the cap on an image's tokens at the budget's
/// ratio, whatever its size, on top of the text.
#[test]
fn char_count_charges_each_image_the_token_cap() {
    assert_eq!(IMAGE_CHARGE_CHARS, 4096 * 4);
    let text_only = AgentMessage::user("héllo");
    assert_eq!(text_only.char_count(), 5);
    let one = AgentMessage::User {
        content: "héllo".into(),
        images: vec![image(b"a")],
    };
    assert_eq!(one.char_count(), 5 + 16_384);
    let three = AgentMessage::User {
        content: String::new(),
        images: vec![image(b"a"), image(b"b"), image(b"a")],
    };
    assert_eq!(three.char_count(), 3 * 16_384);
}

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

/// Only a user message with at least one image carries one.
#[test]
fn only_a_user_message_with_an_image_carries_one() {
    let with = AgentMessage::User {
        content: String::new(),
        images: vec![image(b"a")],
    };
    assert!(with.has_images());
    assert!(!AgentMessage::user("text").has_images());
    let system = AgentMessage::System {
        content: "prompt".into(),
    };
    assert!(!system.has_images());
}
