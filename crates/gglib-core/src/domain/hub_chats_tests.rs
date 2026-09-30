//! The wire shapes of the hub's chats, pinned against the recorded bodies
//! both clients replay: a listing, a chat opened, and a device's turn.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{HubChat, HubChatList, HubChatOpen, HubTurn};
use crate::domain::chat::{Conversation, ConversationSettings, Message, MessageRole};

/// The recorded bodies, by name. The field order is the file's order.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Recorded {
    list: HubChatList,
    open: HubChatOpen,
    turn: HubTurn,
}

fn recorded() -> Recorded {
    let list = HubChatList {
        chats: vec![
            HubChat {
                id: 12,
                title: "Why the build broke".to_owned(),
                model_id: Some(3),
                model: Some("qwen3-8b".to_owned()),
                updated_at: "2026-09-30 09:13:07".to_owned(),
                live_run: Some("chat-5b1e".to_owned()),
            },
            HubChat {
                id: 9,
                title: "New Chat".to_owned(),
                model_id: None,
                model: None,
                updated_at: "2026-09-29 18:02:41".to_owned(),
                live_run: None,
            },
        ],
    };
    let conversation = Conversation {
        id: 12,
        title: "Why the build broke".to_owned(),
        model_id: Some(3),
        system_prompt: Some("You are a helpful assistant.".to_owned()),
        settings: Some(ConversationSettings {
            max_iterations: Some(8),
            ..ConversationSettings::default()
        }),
        created_at: "2026-09-30 09:12:30".to_owned(),
        updated_at: "2026-09-30 09:13:07".to_owned(),
    };
    let messages = vec![
        Message {
            id: 40,
            conversation_id: 12,
            role: MessageRole::User,
            content: "Why did the build break?".to_owned(),
            created_at: "2026-09-30 09:12:31".to_owned(),
            metadata: Some(json!({ "device": "phone-7c2e" })),
        },
        Message {
            id: 41,
            conversation_id: 12,
            role: MessageRole::Assistant,
            content: "A dependency moved.".to_owned(),
            created_at: "2026-09-30 09:13:07".to_owned(),
            metadata: Some(json!({
                "modelName": "qwen3-8b",
                "promptTokens": 812,
                "completionTokens": 96,
                "turnDurationMs": 4100,
                "device": "phone-7c2e",
            })),
        },
    ];
    Recorded {
        list,
        open: HubChatOpen {
            conversation,
            messages,
        },
        turn: HubTurn {
            conversation_id: 12,
            content: "And how do I fix it?".to_owned(),
        },
    }
}

fn recorded_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../contracts/chats/recorded.json")
}

/// The checked-in file is exactly what these types emit. Run with
/// `GGLIB_RECORD_CONTRACTS=1` to rewrite it after a deliberate change.
#[test]
fn recorded_bodies_match_the_checked_in_file() {
    let mut want = serde_json::to_string_pretty(&recorded()).expect("serialise");
    want.push('\n');
    let path = recorded_path();
    if std::env::var_os("GGLIB_RECORD_CONTRACTS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).expect("make contracts/chats");
        std::fs::write(&path, &want).expect("write recorded.json");
    }
    let have = std::fs::read_to_string(&path).expect("read contracts/chats/recorded.json");
    assert!(
        have == want,
        "contracts/chats/recorded.json is stale; rerun with GGLIB_RECORD_CONTRACTS=1\n{want}"
    );
}

/// The file reads back to the same bodies.
#[test]
fn every_recorded_body_decodes_back_to_its_value() {
    let file = std::fs::read_to_string(recorded_path()).expect("read recorded.json");
    let decoded: Recorded = serde_json::from_str(&file).expect("decode recorded.json");
    assert_eq!(
        serde_json::to_value(decoded).unwrap(),
        serde_json::to_value(recorded()).unwrap()
    );
}

/// A chat with nothing to say of its model or a live run leaves the keys
/// out rather than writing `null`.
#[test]
fn a_chats_none_is_an_absent_key() {
    let body = serde_json::to_value(&recorded().list.chats[1]).unwrap();
    let keys: Vec<&str> = body
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys, ["id", "title", "updated_at"]);
}

/// A turn is the chat's id and the message, and reads from nothing more.
#[test]
fn a_turn_is_the_chat_and_the_message() {
    let body = serde_json::to_value(&recorded().turn).unwrap();
    assert_eq!(
        body,
        json!({ "conversation_id": 12, "content": "And how do I fix it?" })
    );
    let missing: Result<HubTurn, _> = serde_json::from_value(json!({ "content": "x" }));
    assert!(missing.is_err());
}
