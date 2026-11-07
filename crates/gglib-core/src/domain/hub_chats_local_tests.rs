//! This machine's turn: a device's turn, key for key, and its folder.

use serde_json::{Value, json};

use super::{HubTurn, LocalTurn};
use crate::domain::Thinking;
use crate::domain::attachment::AttachmentId;

fn every_key() -> LocalTurn {
    LocalTurn {
        conversation_id: 4,
        content: "Draw a red dot".to_owned(),
        images: vec![AttachmentId::of(b"a photo")],
        thinking: Some(Thinking::Off),
        answer_saved: true,
        draw: true,
        image_model: Some(2),
        folder: Some("/tmp/work".to_owned()),
    }
}

fn keys(value: &Value) -> Vec<String> {
    let mut keys: Vec<String> = value.as_object().unwrap().keys().cloned().collect();
    keys.sort();
    keys
}

/// Its keys are a device's turn's and `folder`: a key added to one and not
/// the other fails here.
#[test]
fn its_keys_are_a_devices_turns_and_the_folder() {
    let local = every_key();
    let (turn, _) = local.clone().split();
    let mut want = keys(&serde_json::to_value(&turn).unwrap());
    want.push("folder".to_owned());
    want.sort();
    assert_eq!(keys(&serde_json::to_value(&local).unwrap()), want);
}

/// Split, it is the device's turn with the same values, and its folder.
#[test]
fn split_keeps_every_value() {
    let (turn, folder) = every_key().split();
    assert_eq!(
        turn,
        HubTurn {
            conversation_id: 4,
            content: "Draw a red dot".to_owned(),
            images: vec![AttachmentId::of(b"a photo")],
            thinking: Some(Thinking::Off),
            answer_saved: true,
            draw: true,
            image_model: Some(2),
        }
    );
    assert_eq!(folder.as_deref(), Some("/tmp/work"));
}

/// The smallest body is a chat and its message; nothing unsaid is written.
#[test]
fn the_smallest_body_is_a_chat_and_its_message() {
    let body = json!({ "conversation_id": 4, "content": "Hi" });
    let turn: LocalTurn = serde_json::from_value(body.clone()).unwrap();
    assert_eq!(turn.folder, None);
    assert!(!turn.answer_saved && !turn.draw);
    assert_eq!(serde_json::to_value(&turn).unwrap(), body);
}

/// The old page's keys are refused, not dropped without a word.
#[test]
fn a_key_it_does_not_name_is_refused() {
    for key in ["messages", "port", "tool_filter", "config", "far", "model"] {
        let mut body = json!({ "conversation_id": 4, "content": "Hi" });
        body[key] = json!(null);
        assert!(
            serde_json::from_value::<LocalTurn>(body).is_err(),
            "{key} was taken"
        );
    }
}
