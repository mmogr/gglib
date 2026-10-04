//! A saved message on the wire, and back as an agent message.

use super::*;

fn message(role: MessageRole, images: Vec<AttachmentInfo>) -> Message {
    Message {
        id: 40,
        conversation_id: 12,
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
