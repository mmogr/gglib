//! The agent-message-to-row mapping.

use super::*;
use crate::domain::agent::{AssistantContent, ToolCall};

#[test]
fn system_message_maps_correctly() {
    let msg = AgentMessage::System {
        content: "You are helpful.".into(),
    };
    let out = to_new_message(&msg, 42);
    assert_eq!(out.role, MessageRole::System);
    assert_eq!(out.content, "You are helpful.");
    assert!(out.metadata.is_none());
}

#[test]
fn user_message_maps_correctly() {
    let msg = AgentMessage::User {
        content: "Hello".into(),
        images: Vec::new(),
    };
    let out = to_new_message(&msg, 42);
    assert_eq!(out.role, MessageRole::User);
    assert_eq!(out.content, "Hello");
    assert!(out.metadata.is_none());
}

#[test]
fn assistant_text_only() {
    let msg = AgentMessage::Assistant {
        content: AssistantContent {
            text: Some("The answer is 4.".into()),
            tool_calls: vec![],
        },
    };
    let out = to_new_message(&msg, 42);
    assert_eq!(out.role, MessageRole::Assistant);
    assert_eq!(out.content, "The answer is 4.");
    assert!(out.metadata.is_none());
}

#[test]
fn assistant_with_tool_calls() {
    let msg = AgentMessage::Assistant {
        content: AssistantContent {
            text: Some("Let me check.".into()),
            tool_calls: vec![ToolCall {
                id: "call_1".into(),
                name: "read_file".into(),
                arguments: serde_json::json!({"path": "src/main.rs"}),
            }],
        },
    };
    let out = to_new_message(&msg, 42);
    assert_eq!(out.role, MessageRole::Assistant);
    assert_eq!(out.content, "Let me check.");
    let meta = out.metadata.unwrap();
    let calls = meta["tool_calls"].as_array().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0]["id"], "call_1");
    assert_eq!(calls[0]["name"], "read_file");
}

#[test]
fn assistant_tool_calls_no_text() {
    let msg = AgentMessage::Assistant {
        content: AssistantContent {
            text: None,
            tool_calls: vec![ToolCall {
                id: "call_2".into(),
                name: "list_directory".into(),
                arguments: serde_json::json!({"path": "."}),
            }],
        },
    };
    let out = to_new_message(&msg, 42);
    assert_eq!(out.content, "");
}

#[test]
fn tool_message_maps_correctly() {
    let msg = AgentMessage::Tool {
        tool_call_id: "call_1".into(),
        content: "file contents here".into(),
    };
    let out = to_new_message(&msg, 42);
    assert_eq!(out.role, MessageRole::Tool);
    assert_eq!(out.content, "file contents here");
    let meta = out.metadata.unwrap();
    assert_eq!(meta["tool_call_id"], "call_1");
}

#[test]
fn a_user_message_keeps_its_images_and_no_other_row_has_any() {
    use crate::domain::AttachmentId;

    let images = vec![AttachmentId::of(b"b"), AttachmentId::of(b"a")];
    let msg = AgentMessage::User {
        content: "look".into(),
        images: images.clone(),
    };
    assert_eq!(to_new_message(&msg, 42).images, images);

    let others = [
        AgentMessage::System {
            content: "s".into(),
        },
        AgentMessage::Assistant {
            content: AssistantContent {
                text: Some("a".into()),
                tool_calls: vec![],
            },
        },
        AgentMessage::Tool {
            tool_call_id: "call_1".into(),
            content: "t".into(),
        },
    ];
    for other in &others {
        assert!(to_new_message(other, 42).images.is_empty());
    }
}
