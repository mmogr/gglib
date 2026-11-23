//! A conversation with an image, exported to the file the chat page saves,
//! and never over a file by accident.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use gglib_core::domain::attachment::{AttachmentBlob, AttachmentId, AttachmentInfo};
use gglib_core::domain::chat::{MessageRole, NewConversation, NewMessage};
use gglib_core::ports::attachment_store::{AttachmentError, AttachmentStore};
use serde_json::{Value, json};

use super::*;

/// A PNG's signature and `IHDR` at `width` by `height`.
fn png(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13];
    bytes.extend(b"IHDR");
    bytes.extend(width.to_be_bytes());
    bytes.extend(height.to_be_bytes());
    bytes.extend([8, 6, 0, 0, 0]);
    bytes
}

/// A core over an in-memory database, holding a chat whose question carries
/// a stored image; the core, and the chat's id.
async fn chat_with_an_image() -> (AppCore, i64) {
    let pool = gglib_db::setup_test_database().await.expect("in-memory DB");
    let app = AppCore::bare(gglib_db::CoreFactory::build_repos(pool));
    let image = app.attachments().ingest(&png(4, 2)).await.unwrap().info;
    let history = app.chat_history();
    let id = history
        .create_conversation(NewConversation {
            title: "A screenshot".into(),
            system_prompt: Some("You help.".into()),
            ..NewConversation::default()
        })
        .await
        .unwrap();
    for (role, content, images) in [
        (MessageRole::User, "What is this?", vec![image.id]),
        (MessageRole::Assistant, "A dot.", Vec::new()),
    ] {
        let row = NewMessage {
            conversation_id: id,
            role,
            content: content.to_owned(),
            metadata: None,
            images,
        };
        history.save_message(row).await.unwrap();
    }
    (app, id)
}

#[tokio::test]
async fn a_chat_with_an_image_exports_to_the_file_the_page_saves() {
    let (app, id) = chat_with_an_image().await;
    let dir = tempfile::tempdir().unwrap();

    let (saved, export) = save(&app, id, Some(dir.path()), false).await.unwrap();

    assert_eq!(saved, dir.path().join(format!("conversation-{id}.json")));
    let text = std::fs::read_to_string(&saved).unwrap();
    assert_eq!(
        text,
        export.text(),
        "the daemon's answer, as the page saves it"
    );
    let file: Value = serde_json::from_str(&text).unwrap();
    // The page's `conversation` is the chat as the sidebar lists it.
    let listed = app.chat_history().list_conversations().await.unwrap();
    assert_eq!(
        file["conversation"],
        serde_json::to_value(&listed[0]).unwrap()
    );
    let image = &file["messages"][0]["images"][0];
    let id = gglib_core::domain::AttachmentId::parse(image["id"].as_str().unwrap()).unwrap();
    let blob = app.attachments().blob(&id).await.unwrap();
    assert_eq!(image["data_url"], json!(blob.data_url()));
    assert_eq!(
        (image["width"].clone(), image["height"].clone()),
        (json!(4), json!(2))
    );
    assert_eq!(file["messages"][1]["content"], "A dot.");
    assert!(file["messages"][1].get("images").is_none());
    assert_eq!(export.unread(), 0);
}

#[tokio::test]
async fn an_export_never_replaces_a_file_without_force() {
    let (app, id) = chat_with_an_image().await;
    let dir = tempfile::tempdir().unwrap();
    let named = dir.path().join("mine.json");
    std::fs::write(&named, "keep me").unwrap();

    let err = save(&app, id, Some(&named), false).await.unwrap_err();
    assert!(
        err.to_string().contains("already exists; pass --force"),
        "{err}"
    );
    assert_eq!(std::fs::read_to_string(&named).unwrap(), "keep me");

    let (saved, export) = save(&app, id, Some(&named), true).await.unwrap();
    assert_eq!(saved, named);
    assert_eq!(std::fs::read_to_string(&named).unwrap(), export.text());
}

#[tokio::test]
async fn an_id_no_conversation_has_is_refused_and_names_the_listing() {
    let (app, _) = chat_with_an_image().await;
    let dir = tempfile::tempdir().unwrap();

    let err = save(&app, 99, Some(dir.path()), false).await.unwrap_err();

    assert_eq!(
        err.to_string(),
        "No conversation has the ID 99; `gglib chat history` lists them."
    );
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

/// A store whose bytes cannot be read, counting each time they are asked.
struct Unreadable(Arc<AtomicUsize>);

#[async_trait]
impl AttachmentStore for Unreadable {
    async fn put(&self, _: &AttachmentInfo, _: &[u8]) -> Result<(), AttachmentError> {
        unreachable!("an export reads the store")
    }

    async fn info(&self, _: &AttachmentId) -> Result<Option<AttachmentInfo>, AttachmentError> {
        unreachable!("an export reads the rows' info")
    }

    async fn size(&self, _: &AttachmentId) -> Result<Option<usize>, AttachmentError> {
        unreachable!("an export reads the bytes")
    }

    async fn blob(&self, _: &AttachmentId) -> Result<Option<AttachmentBlob>, AttachmentError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Err(AttachmentError::Storage("the disk is gone".to_owned()))
    }

    async fn ids_starting_with(&self, _: &str) -> Result<Vec<AttachmentId>, AttachmentError> {
        unreachable!("an export names each image by its whole id")
    }
}

#[tokio::test]
async fn an_image_the_store_cannot_give_is_in_the_file_by_its_id_and_asked_for_once() {
    let pool = gglib_db::setup_test_database().await.expect("in-memory DB");
    let stored = AppCore::bare(gglib_db::CoreFactory::build_repos(pool.clone()));
    let image = stored.attachments().ingest(&png(4, 2)).await.unwrap().info;
    let history = stored.chat_history();
    let id = history
        .create_conversation(NewConversation::default())
        .await
        .unwrap();
    for role in [MessageRole::User, MessageRole::Tool] {
        let row = NewMessage {
            conversation_id: id,
            role,
            content: "[image]".to_owned(),
            metadata: None,
            images: vec![image.id.clone()],
        };
        history.save_message(row).await.unwrap();
    }
    // The same rows, over a store that cannot give the bytes.
    let asked = Arc::new(AtomicUsize::new(0));
    let app = AppCore::bare(gglib_core::ports::Repos {
        attachments: Arc::new(Unreadable(Arc::clone(&asked))),
        ..gglib_db::CoreFactory::build_repos(pool)
    });
    let dir = tempfile::tempdir().unwrap();

    let (saved, export) = save(&app, id, Some(dir.path()), false).await.unwrap();

    let file: Value = serde_json::from_str(&std::fs::read_to_string(saved).unwrap()).unwrap();
    for row in 0..2 {
        let kept = &file["messages"][row]["images"][0];
        assert_eq!(kept["id"], json!(image.id.as_str()));
        assert!(kept.get("data_url").is_none(), "{kept}");
    }
    assert_eq!(export.unread(), 1);
    assert_eq!(
        asked.load(Ordering::SeqCst),
        1,
        "each image is asked for once"
    );
}
