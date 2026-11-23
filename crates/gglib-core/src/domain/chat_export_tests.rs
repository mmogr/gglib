//! The export file's shape, written to `contracts/chats/export.json` for the
//! page, which saves what the daemon answers.

use std::collections::HashMap;
use std::path::PathBuf;

use serde_json::{Value, json};

use super::*;

/// An image's info, its id `fill` repeated.
fn image(fill: char, mime: &str, width: u32, height: u32) -> AttachmentInfo {
    AttachmentInfo {
        id: AttachmentId::parse(&fill.to_string().repeat(64)).unwrap(),
        mime: mime.to_owned(),
        width,
        height,
    }
}

fn conversation() -> Conversation {
    Conversation {
        id: 7,
        title: "Why the build broke".to_owned(),
        model_id: Some(3),
        system_prompt: Some("You help.".to_owned()),
        settings: None,
        created_at: "2026-09-30 09:12:30".to_owned(),
        updated_at: "2026-09-30 09:13:07".to_owned(),
        branch_of: None,
        lineage_id: None,
    }
}

fn row(
    id: i64,
    role: MessageRole,
    content: &str,
    metadata: Option<Value>,
    images: &[AttachmentInfo],
) -> Message {
    Message {
        id,
        conversation_id: 7,
        role,
        content: content.to_owned(),
        created_at: "2026-09-30 09:12:30".to_owned(),
        metadata,
        images: images.to_vec(),
        origin_id: None,
    }
}

/// A screenshot the user asked about, a reply with none, and a tool's row
/// with the same screenshot and a picture the store no longer has.
fn chat() -> (Conversation, Vec<Message>, HashMap<AttachmentId, String>) {
    let shot = image('a', "image/png", 2560, 1440);
    let drawn = image('b', "image/jpeg", 512, 512);
    let rows = vec![
        row(
            1,
            MessageRole::User,
            "What broke?",
            None,
            std::slice::from_ref(&shot),
        ),
        row(
            2,
            MessageRole::Assistant,
            "The linker.",
            Some(json!({ "thinking": "Read the log.", "thinkingDurationSeconds": 2.0 })),
            &[],
        ),
        row(
            3,
            MessageRole::Tool,
            "[image]",
            None,
            &[shot.clone(), drawn],
        ),
    ];
    let urls = HashMap::from([(shot.id, "data:image/png;base64,cG5n".to_owned())]);
    (conversation(), rows, urls)
}

#[test]
fn each_image_carries_its_bytes_and_one_the_store_lacks_its_id_alone() {
    let (conversation, rows, urls) = chat();
    let export = ChatExport::new(conversation, rows, &urls);
    let file: Value = serde_json::from_str(&export.text()).unwrap();

    assert_eq!(
        file["messages"][0]["images"],
        json!([{
            "id": "a".repeat(64), "mime": "image/png", "width": 2560, "height": 1440,
            "data_url": "data:image/png;base64,cG5n",
        }])
    );
    assert_eq!(file["messages"][2]["images"][1]["id"], "b".repeat(64));
    assert!(file["messages"][2]["images"][1].get("data_url").is_none());
    assert_eq!(export.unread(), 1, "the missing picture, counted once");
    assert_eq!(export.file_name(), "conversation-7.json");
}

#[test]
fn a_message_reads_in_the_file_as_the_thread_serves_it() {
    let (conversation, rows, urls) = chat();
    let served = serde_json::to_value(&rows[1]).unwrap();
    let export = ChatExport::new(conversation.clone(), rows, &urls);

    assert_eq!(serde_json::to_value(&export.messages[1]).unwrap(), served);
    assert_eq!(
        serde_json::to_value(&export.conversation).unwrap(),
        serde_json::to_value(&conversation).unwrap(),
    );
}

#[test]
fn an_image_the_store_lacks_is_counted_once_however_many_rows_carry_it() {
    let (conversation, rows, _) = chat();
    let export = ChatExport::new(conversation, rows, &HashMap::new());
    assert_eq!(export.unread(), 2);
    assert_eq!(
        unread_sentence(2).as_deref(),
        Some("2 images could not be read. They are in the file by their ids only.")
    );
    assert_eq!(unread_sentence(0), None);
}

/// The file [`chat`] exports to, as the daemon serves it and the CLI writes
/// it, the name it is saved by, and what a client says of the image it could
/// not read.
fn export_contract() -> Value {
    let (conversation, rows, urls) = chat();
    let export = ChatExport::new(conversation, rows, &urls);
    json!({
        "text": export.text(),
        "name": export.file_name(),
        "unread": unread_sentence(export.unread()),
    })
}

fn contract_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../contracts/chats/export.json")
}

/// The page saves what the daemon answers, in the same text.
/// `GGLIB_RECORD_CONTRACTS=1` rewrites the file after a deliberate change.
#[test]
fn the_export_contract_is_current() {
    let want = serde_json::to_string_pretty(&export_contract()).unwrap() + "\n";
    let path = contract_path();
    if std::env::var_os("GGLIB_RECORD_CONTRACTS").is_some() {
        std::fs::write(&path, &want).expect("write export.json");
    }
    let have = std::fs::read_to_string(&path).expect("read contracts/chats/export.json");
    assert_eq!(
        have, want,
        "contracts/chats/export.json is stale; rerun with GGLIB_RECORD_CONTRACTS=1"
    );
}
