//! The wire shapes of the hub's chats, pinned against the recorded bodies
//! both clients replay: a listing, a chat opened (a finished reply with how
//! it was made, then one that was stopped), a device's turn, a turn with an
//! image, the answer to the upload that image was sent by, and a turn that
//! turns thinking off (`hub_chats_thinking_tests` reads that one).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{HubChat, HubChatList, HubChatOpen, HubTurn};
use crate::domain::Thinking;
use crate::domain::attachment::{AttachmentId, AttachmentInfo, AttachmentUpload};
use crate::domain::chat::{Conversation, ConversationSettings, Message, MessageRole};

/// The recorded bodies, by name. The field order is the file's order.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Recorded {
    list: HubChatList,
    pub(super) open: HubChatOpen,
    pub(super) turn: HubTurn,
    upload: AttachmentUpload,
    image_turn: HubTurn,
    pub(super) thinking_turn: HubTurn,
}

/// The image the recorded chat carries: a 1280x720 PNG, as an upload answers
/// it. The id is the hash of this text, standing in for the file's bytes.
fn screenshot() -> AttachmentInfo {
    AttachmentInfo {
        id: AttachmentId::of(b"contracts/chats: a screenshot of the failed build"),
        mime: "image/png".to_owned(),
        width: 1280,
        height: 720,
    }
}

/// A turn on the recorded chat that says `content`, with no image and no
/// Thinking choice.
fn turn(content: &str) -> HubTurn {
    HubTurn {
        conversation_id: 12,
        content: content.to_owned(),
        images: Vec::new(),
        thinking: None,
    }
}

pub(super) fn recorded() -> Recorded {
    let list = HubChatList {
        chats: vec![
            HubChat {
                id: 12,
                title: "Why the build broke".to_owned(),
                model_id: Some(3),
                model: Some("qwen3-8b".to_owned()),
                updated_at: "2026-09-30 09:14:21".to_owned(),
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
            thinking: Some(Thinking::Off),
            ..ConversationSettings::default()
        }),
        created_at: "2026-09-30 09:12:30".to_owned(),
        updated_at: "2026-09-30 09:14:21".to_owned(),
    };
    let messages = vec![
        Message {
            id: 40,
            conversation_id: 12,
            role: MessageRole::User,
            content: "Why did the build break?".to_owned(),
            created_at: "2026-09-30 09:12:31".to_owned(),
            metadata: Some(json!({ "device": "phone-7c2e" })),
            images: vec![screenshot()],
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
                "finishReason": "stop",
                "contextSize": 8192,
            })),
            images: Vec::new(),
        },
        Message {
            id: 42,
            conversation_id: 12,
            role: MessageRole::User,
            content: "And how do I fix it?".to_owned(),
            created_at: "2026-09-30 09:14:02".to_owned(),
            metadata: Some(json!({ "device": "phone-7c2e" })),
            images: Vec::new(),
        },
        // Stopped before its stream ended: no counts, no reading.
        Message {
            id: 43,
            conversation_id: 12,
            role: MessageRole::Assistant,
            content: "Pin the".to_owned(),
            created_at: "2026-09-30 09:14:21".to_owned(),
            metadata: Some(json!({ "incomplete": true })),
            images: Vec::new(),
        },
    ];
    Recorded {
        list,
        open: HubChatOpen {
            conversation,
            messages,
        },
        turn: turn("And how do I fix it?"),
        upload: AttachmentUpload {
            info: screenshot(),
            image_tokens: 920,
        },
        image_turn: HubTurn {
            images: vec![screenshot().id],
            ..turn("What does this error mean?")
        },
        thinking_turn: HubTurn {
            thinking: Some(Thinking::Off),
            ..turn("Answer in one line.")
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
    for key in ["model", "messages", "replace_from"] {
        let mut more = body.clone();
        more[key] = json!(null);
        assert!(serde_json::from_value::<HubTurn>(more).is_err(), "{key}");
    }
}

/// A turn with images names each by its id, in order, beside the message;
/// one with none leaves the key out, and reads back from a body without it.
#[test]
fn a_turn_names_its_images_by_id() {
    let id = screenshot().id;
    let body = serde_json::to_value(&recorded().image_turn).unwrap();
    assert_eq!(
        body,
        json!({
            "conversation_id": 12,
            "content": "What does this error mean?",
            "images": [id.as_str()],
        })
    );
    let read: HubTurn = serde_json::from_value(body.clone()).unwrap();
    assert_eq!(read.images, [id]);

    let bare: HubTurn =
        serde_json::from_value(json!({ "conversation_id": 12, "content": "x" })).unwrap();
    assert!(bare.images.is_empty());

    let mut more = body;
    more["messages"] = json!([]);
    assert!(serde_json::from_value::<HubTurn>(more).is_err());
    let not_an_id = json!({ "conversation_id": 12, "content": "x", "images": ["shot.png"] });
    assert!(serde_json::from_value::<HubTurn>(not_an_id).is_err());
}

/// An opened chat's row carries its images without their bytes, and a row
/// with none leaves the key out. The upload's answer is the same facts and
/// the estimate, flat.
#[test]
fn a_row_carries_its_images_and_an_upload_answers_the_same_facts() {
    let recorded = recorded();
    let rows = serde_json::to_value(&recorded.open.messages).unwrap();
    let image = json!({
        "id": screenshot().id.as_str(),
        "mime": "image/png",
        "width": 1280,
        "height": 720,
    });
    assert_eq!(rows[0]["images"], json!([image]));
    assert!(rows[1].get("images").is_none());

    let mut upload = image;
    upload["image_tokens"] = json!(920);
    assert_eq!(serde_json::to_value(&recorded.upload).unwrap(), upload);
    assert_eq!(
        recorded.upload.image_tokens,
        crate::request_pipeline::estimate_image_tokens(1280, 720)
    );
}

/// The recorded reply says how it was made under the keys a saved row uses,
/// its context's size and why it stopped among them, and no count of
/// messages trimmed: only one message came before it, so none was. The
/// reply stopped after it carries its mark and no figure.
#[test]
fn the_recorded_reply_carries_its_reading_and_the_stopped_one_none() {
    use crate::domain::agent::{INCOMPLETE_KEY, MADE_KEYS as K};

    let rows = recorded().open.messages;
    let reply = rows[1].metadata.as_ref().unwrap();
    for key in [K.prompt_tokens, K.completion_tokens, K.context_size] {
        assert!(reply[key].is_u64(), "{key}");
    }
    assert!(reply.get(K.trimmed_messages).is_none(), "none was trimmed");
    assert_eq!(reply[K.finish_reason], "stop");
    assert_eq!(rows[3].role, MessageRole::Assistant);
    assert_eq!(rows[3].metadata, Some(json!({ INCOMPLETE_KEY: true })));
}
