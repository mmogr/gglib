//! The agent-message-to-row mapping, and a saved chat read back.

use super::*;
use crate::domain::agent::{AssistantContent, ToolCall};

/// A saved row saying `content` as `role`.
fn row(role: MessageRole, content: &str) -> Message {
    Message {
        id: 1,
        conversation_id: 42,
        origin_id: None,
        role,
        content: content.to_owned(),
        created_at: String::new(),
        metadata: None,
        images: Vec::new(),
    }
}

/// A message as its role and what it says.
fn said_by(message: &AgentMessage) -> (&'static str, &str) {
    match message {
        AgentMessage::System { content } => ("system", content),
        AgentMessage::User { content, .. } => ("user", content),
        AgentMessage::Assistant { content } => ("assistant", content.text.as_deref().unwrap_or("")),
        AgentMessage::Tool { content, .. } => ("tool", content),
    }
}

/// Each message as its role and what it says.
fn said(messages: &[AgentMessage]) -> Vec<(&'static str, &str)> {
    messages.iter().map(said_by).collect()
}

/// The prompt first, trimmed, then the rows in order: what the next turn
/// starts from.
#[test]
fn a_saved_chat_reads_back_as_its_prompt_then_its_rows() {
    let rows = [
        row(MessageRole::User, "first"),
        row(MessageRole::Assistant, "answer"),
        row(MessageRole::Tool, "result"),
    ];

    let history = saved_history(Some("  Be brief.\n"), &rows);

    assert_eq!(
        said(&history),
        [
            ("system", "Be brief."),
            ("user", "first"),
            ("assistant", "answer"),
            ("tool", "result")
        ]
    );
}

/// A chat saved when the prompt was kept as a message holds a system row.
/// It is not sent: the prompt is the conversation's, and with none, none is
/// sent, wherever in the rows the system one sits.
#[test]
fn a_saved_system_row_is_never_sent() {
    let rows = [
        row(MessageRole::System, "OLD-PROMPT"),
        row(MessageRole::User, "first"),
        row(MessageRole::System, "LATER-PROMPT"),
        row(MessageRole::Assistant, "answer"),
    ];

    let with_a_prompt = saved_history(Some("Be brief."), &rows);
    let with_none = saved_history(None, &rows);

    assert_eq!(
        said(&with_a_prompt),
        [
            ("system", "Be brief."),
            ("user", "first"),
            ("assistant", "answer")
        ]
    );
    assert_eq!(
        said(&with_none),
        [("user", "first"), ("assistant", "answer")]
    );
}

/// A prompt that is empty once trimmed is no prompt: no empty system
/// message leads the history.
#[test]
fn a_blank_prompt_sends_no_system_message() {
    let rows = [row(MessageRole::User, "first")];

    for blank in [Some(""), Some("  \n\t"), None] {
        assert_eq!(said(&saved_history(blank, &rows)), [("user", "first")]);
    }
    assert!(saved_history(Some(" "), &[]).is_empty());
}

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
