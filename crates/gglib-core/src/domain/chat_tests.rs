//! A saved message on the wire, and back as an agent message; and the
//! machine a stored conversation ran on.

use super::*;

/// A stored conversation with this `model_id`, and this model in its
/// settings.
fn conversation(model_id: Option<i64>, model: Option<ModelRef>) -> Conversation {
    Conversation {
        id: 12,
        title: "t".to_owned(),
        branch_of: None,
        lineage_id: None,
        model_id,
        system_prompt: None,
        settings: model.map(|model| ConversationSettings {
            model: Some(model),
            ..ConversationSettings::default()
        }),
        created_at: String::new(),
        updated_at: String::new(),
    }
}

/// Model 3 of the paired machine.
fn far() -> ModelRef {
    let fingerprint = "0123456789ab".to_owned();
    ModelRef {
        machine: Machine::Paired { fingerprint },
        id: 3,
    }
}

/// The stored model's machine is the conversation's, whatever `model_id`
/// says beside it: a row whose two fields disagree is its settings'.
#[test]
fn a_conversation_ran_on_its_stored_models_machine() {
    let here = ModelRef {
        machine: Machine::Local,
        id: 3,
    };

    assert_eq!(
        conversation(Some(3), Some(here.clone())).machine(),
        Some(Machine::Local)
    );
    assert_eq!(
        conversation(None, Some(here)).machine(),
        Some(Machine::Local)
    );
    assert_eq!(
        conversation(None, Some(far())).machine(),
        Some(far().machine)
    );
    assert_eq!(
        conversation(Some(3), Some(far())).machine(),
        Some(far().machine)
    );
}

/// A row from before a conversation stored its model holds only a
/// `model_id`, which is this catalogue's: it ran here. One that stores
/// neither, or settings that name no model, ran nowhere that is recorded.
#[test]
fn a_conversation_with_only_a_model_id_ran_on_this_machine() {
    let mut named_only = conversation(None, None);
    named_only.settings = Some(ConversationSettings {
        model_name: Some("qwen3".to_owned()),
        ..ConversationSettings::default()
    });

    assert_eq!(conversation(Some(3), None).machine(), Some(Machine::Local));
    assert_eq!(conversation(None, None).machine(), None);
    assert_eq!(named_only.machine(), None);
    named_only.model_id = Some(3);
    assert_eq!(named_only.machine(), Some(Machine::Local));
}

fn message(role: MessageRole, images: Vec<AttachmentInfo>) -> Message {
    Message {
        id: 40,
        conversation_id: 12,
        origin_id: None,
        role,
        content: "look".to_owned(),
        created_at: "2026-09-30 09:12:31".to_owned(),
        metadata: None,
        images,
    }
}

fn image(bytes: &[u8], width: u32) -> AttachmentInfo {
    AttachmentInfo {
        id: AttachmentId::of(bytes),
        mime: "image/png".to_owned(),
        width,
        height: 100,
    }
}

/// A message with no image is, byte for byte, what it was before a message
/// could carry one: no `images` key.
#[test]
fn a_message_with_no_images_serialises_with_no_images_key() {
    assert_eq!(
        serde_json::to_string(&message(MessageRole::User, Vec::new())).unwrap(),
        r#"{"id":40,"conversation_id":12,"role":"user","content":"look","created_at":"2026-09-30 09:12:31"}"#
    );
}

#[test]
fn a_message_with_no_images_key_deserialises_to_none() {
    let json = r#"{"id":40,"conversation_id":12,"role":"user","content":"look","created_at":"2026-09-30 09:12:31"}"#;
    let decoded: Message = serde_json::from_str(json).unwrap();
    assert!(decoded.images.is_empty());
}

#[test]
fn a_message_lists_its_images_as_facts_in_order() {
    let (a, b) = (image(b"a", 1), image(b"b", 2));
    let wire =
        serde_json::to_value(message(MessageRole::User, vec![b.clone(), a.clone()])).unwrap();
    assert_eq!(
        wire["images"],
        serde_json::json!([
            {"id": b.id.as_str(), "mime": "image/png", "width": 2, "height": 100},
            {"id": a.id.as_str(), "mime": "image/png", "width": 1, "height": 100},
        ])
    );
    let decoded: Message = serde_json::from_value(wire).unwrap();
    assert_eq!(decoded.images, [b, a]);
}

#[test]
fn a_user_message_resumes_with_its_images_by_id_in_order() {
    let (a, b) = (image(b"a", 1), image(b"b", 2));
    let resumed = message(MessageRole::User, vec![b.clone(), a.clone()]).to_agent_message();
    let AgentMessage::User { content, images } = resumed else {
        panic!("expected AgentMessage::User");
    };
    assert_eq!(content, "look");
    assert_eq!(images, [b.id, a.id]);

    let AgentMessage::User { images, .. } =
        message(MessageRole::User, Vec::new()).to_agent_message()
    else {
        panic!("expected AgentMessage::User");
    };
    assert!(images.is_empty());
}

/// A tool row's images are the user's to see: the message it resumes as,
/// which is what the model is sent, is the same with them as without.
#[test]
fn a_tool_message_resumes_without_its_images() {
    let tool_row = |images| Message {
        metadata: Some(serde_json::json!({ "tool_call_id": "c1" })),
        ..message(MessageRole::Tool, images)
    };
    let with = tool_row(vec![image(b"a", 1)]).to_agent_message();
    let without = tool_row(Vec::new()).to_agent_message();

    assert_eq!(
        serde_json::to_value(&with).unwrap(),
        serde_json::to_value(&without).unwrap()
    );
    let AgentMessage::Tool {
        tool_call_id,
        content,
    } = with
    else {
        panic!("expected AgentMessage::Tool");
    };
    assert_eq!((tool_call_id.as_str(), content.as_str()), ("c1", "look"));
}
