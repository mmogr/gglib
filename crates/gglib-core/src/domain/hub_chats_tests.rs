//! The wire shapes of the hub's chats, pinned against the recorded bodies
//! both clients replay: a listing, a chat opened (a finished reply with how
//! it was made, then one that was stopped), the rows of a reply whose tool
//! made an image, a device's turn, a turn with an image, the answer to the
//! upload that image was sent by, a turn that turns thinking off
//! (`hub_chats_thinking_tests` reads that one), and a branch with the change
//! that made it and the turn that answers it (`hub_chats_branch_tests`, which
//! also builds them).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::json;

use super::hub_chats_branch_tests as branch;
use super::{HubChat, HubChatList, HubChatOpen, HubTurn};
use crate::domain::Thinking;
use crate::domain::agent::ToolCall;
use crate::domain::attachment::{AttachmentId, AttachmentInfo, AttachmentUpload};
use crate::domain::branching::{ChatChange, ChatChanged};
use crate::domain::chat::{Conversation, ConversationSettings, Message, MessageRole};

/// The recorded bodies, by name. The field order is the file's order.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Recorded {
    pub(super) list: HubChatList,
    pub(super) open: HubChatOpen,
    tool_reply: Vec<Message>,
    pub(super) turn: HubTurn,
    upload: AttachmentUpload,
    image_turn: HubTurn,
    pub(super) thinking_turn: HubTurn,
    pub(super) branch_open: HubChatOpen,
    pub(super) change: ChatChange,
    pub(super) changed: ChatChanged,
    pub(super) answer_turn: HubTurn,
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

/// The image the recorded tool made: a 1024x1024 PNG, the same image
/// `contracts/runs/recorded.json`'s `tool_reply` frames carry. The id is the
/// hash of this text, standing in for the image's bytes.
fn drawing() -> AttachmentInfo {
    AttachmentInfo {
        id: AttachmentId::of(b"contracts: a red dot a tool drew"),
        mime: "image/png".to_owned(),
        width: 1024,
        height: 1024,
    }
}

/// The rows of a reply whose tool made an image, as an opened chat carries
/// them: the assistant row that called the tool, the tool row with the
/// image, and the answer. They sit beside `open` rather than in it, so the
/// opened chat's four rows stay as both clients replay them.
fn tool_reply() -> Vec<Message> {
    let call = ToolCall {
        id: "call-draw-1".to_owned(),
        name: "draw".to_owned(),
        arguments: json!({ "prompt": "a red dot" }),
    };
    let row = |id, role, content: &str, created_at: &str| Message {
        id,
        conversation_id: 12,
        role,
        content: content.to_owned(),
        created_at: created_at.to_owned(),
        metadata: None,
        images: Vec::new(),
        origin_id: None,
    };
    vec![
        Message {
            metadata: Some(json!({ "tool_calls": [call] })),
            ..row(44, MessageRole::Assistant, "", "2026-09-30 09:15:02")
        },
        Message {
            metadata: Some(json!({ "tool_call_id": "call-draw-1" })),
            images: vec![drawing()],
            ..row(
                45,
                MessageRole::Tool,
                "[image 1024x1024 PNG stored]",
                "2026-09-30 09:15:09",
            )
        },
        row(
            46,
            MessageRole::Assistant,
            "Here is a red dot.",
            "2026-09-30 09:15:11",
        ),
    ]
}

/// A turn on the recorded chat that says `content`, with no image and no
/// Thinking choice.
fn turn(content: &str) -> HubTurn {
    HubTurn {
        conversation_id: 12,
        content: content.to_owned(),
        images: Vec::new(),
        thinking: None,
        answer_saved: false,
        draw: false,
    }
}

/// A row of the recorded chat, saved at `at` on its day, with `metadata`
/// and no image.
fn row(
    id: i64,
    role: MessageRole,
    content: &str,
    at: &str,
    metadata: serde_json::Value,
) -> Message {
    Message {
        id,
        conversation_id: 12,
        role,
        content: content.to_owned(),
        created_at: format!("2026-09-30 {at}"),
        metadata: Some(metadata),
        images: Vec::new(),
        origin_id: None,
    }
}

/// The listing: a chat with a live run, one with nothing to say of its
/// model, and a branch of the first.
fn listed() -> HubChatList {
    HubChatList {
        chats: vec![
            HubChat {
                id: 12,
                title: "Why the build broke".to_owned(),
                model_id: Some(3),
                model: Some("qwen3-8b".to_owned()),
                updated_at: "2026-09-30 09:14:21".to_owned(),
                live_run: Some("chat-5b1e".to_owned()),
                branch_of: None,
            },
            HubChat {
                id: 9,
                title: "New Chat".to_owned(),
                model_id: None,
                model: None,
                updated_at: "2026-09-29 18:02:41".to_owned(),
                live_run: None,
                branch_of: None,
            },
            branch::listed(),
        ],
    }
}

pub(super) fn recorded() -> Recorded {
    let list = listed();
    let conversation = Conversation {
        id: 12,
        title: "Why the build broke".to_owned(),
        branch_of: None,
        lineage_id: None,
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
            images: vec![screenshot()],
            ..row(
                40,
                MessageRole::User,
                "Why did the build break?",
                "09:12:31",
                json!({ "device": "phone-7c2e" }),
            )
        },
        row(
            41,
            MessageRole::Assistant,
            "A dependency moved.",
            "09:13:07",
            json!({
                "modelName": "qwen3-8b",
                "promptTokens": 812,
                "completionTokens": 96,
                "turnDurationMs": 4100,
                "device": "phone-7c2e",
                "finishReason": "stop",
                "contextSize": 8192,
            }),
        ),
        row(
            42,
            MessageRole::User,
            "And how do I fix it?",
            "09:14:02",
            json!({ "device": "phone-7c2e" }),
        ),
        // Stopped before its stream ended: no counts, no reading.
        row(
            43,
            MessageRole::Assistant,
            "Pin the",
            "09:14:21",
            json!({ "incomplete": true }),
        ),
    ];
    Recorded {
        list,
        open: HubChatOpen {
            conversation,
            messages,
            points: Vec::new(),
            answerable: false,
        },
        tool_reply: tool_reply(),
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
        branch_open: branch::opened(),
        change: branch::change(),
        changed: branch::changed(),
        answer_turn: branch::answer_turn(),
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

/// A tool row carries the images its tool made as a user row does, by id
/// with their facts and without their bytes, beside the id of the call it
/// answers; its text names each image. The rows around it have none.
#[test]
fn a_tool_row_carries_the_images_its_tool_made() {
    let rows = serde_json::to_value(recorded().tool_reply).unwrap();
    let image = drawing();
    assert_eq!(
        rows[1],
        json!({
            "id": 45,
            "conversation_id": 12,
            "role": "tool",
            "content": "[image 1024x1024 PNG stored]",
            "created_at": "2026-09-30 09:15:09",
            "metadata": { "tool_call_id": "call-draw-1" },
            "images": [{
                "id": image.id.as_str(),
                "mime": "image/png",
                "width": 1024,
                "height": 1024,
            }],
        })
    );
    assert_eq!(rows[0]["metadata"]["tool_calls"][0]["id"], "call-draw-1");
    assert!(rows[0].get("images").is_none() && rows[2].get("images").is_none());
}

/// Draw travels only when pressed: `true` is said, `false` is left out of
/// the body, and a body without the key reads as not pressed.
#[test]
fn a_turn_says_draw_only_when_it_was_pressed() {
    let pressed = HubTurn {
        draw: true,
        ..turn("a fox")
    };
    assert_eq!(
        serde_json::to_value(&pressed).unwrap(),
        serde_json::json!({ "conversation_id": 12, "content": "a fox", "draw": true })
    );
    let plain = serde_json::to_value(turn("hi")).unwrap();
    assert!(plain.get("draw").is_none(), "{plain}");
    let read: HubTurn = serde_json::from_str(r#"{"conversation_id":12,"content":"hi"}"#).unwrap();
    assert!(!read.draw);
}
