//! An image a paired device sends is stored by the hub's one ingest, shown
//! on the row that names it without its bytes, and read back by its id. A
//! picture it keeps beside one a tool drew joins that picture's tool row.

use std::sync::Arc;

use gglib_core::domain::AttachmentId;
use gglib_core::domain::chat::{MessageRole, NewConversation, NewMessage};
use gglib_core::ports::{AttachmentError, HubChatsError, HubChatsPort};
use gglib_core::request_pipeline::estimate_image_tokens;

use super::HubChats;
use crate::runs::test_executor::registry;
use crate::test_support::test_core;

/// A PNG's signature and `IHDR`, 640 by 480: all the store reads of one.
fn png() -> Vec<u8> {
    png_of(640, 480)
}

/// The same, `width` by `height`.
fn png_of(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    bytes.extend([0, 0, 0, 13]);
    bytes.extend(b"IHDR");
    bytes.extend(width.to_be_bytes());
    bytes.extend(height.to_be_bytes());
    bytes.extend([8, 6, 0, 0, 0, 0, 0, 0, 0]);
    bytes
}

#[tokio::test]
async fn a_devices_image_is_stored_shown_on_its_row_and_read_back() {
    let core = test_core().await;
    let (runs, _, _) = registry();
    let chats = HubChats::new(Arc::clone(&core), &Arc::new(runs));
    let image = png();

    let stored = chats.attach(&image).await.unwrap();

    assert_eq!(stored.info.id, AttachmentId::of(&image));
    assert_eq!(
        (
            stored.info.mime.as_str(),
            stored.info.width,
            stored.info.height
        ),
        ("image/png", 640, 480)
    );
    assert_eq!(stored.image_tokens, estimate_image_tokens(640, 480));

    let history = core.chat_history();
    let id = history
        .create_conversation(NewConversation {
            title: "t".to_owned(),
            ..NewConversation::default()
        })
        .await
        .unwrap();
    let row = NewMessage {
        conversation_id: id,
        role: MessageRole::User,
        content: "what is this?".to_owned(),
        metadata: None,
        images: vec![stored.info.id.clone()],
    };
    history.save_message(row).await.unwrap();

    let open = chats.open(id).await.unwrap();
    assert_eq!(open.messages[0].images, vec![stored.info.clone()]);

    let read = chats.attachment(&stored.info.id).await.unwrap();
    assert_eq!(read.mime, "image/png");
    assert!(read.data == image, "the bytes are the ones that were sent");
}

/// What the ingest refuses, and an id it never stored, come back as the
/// refusals a client matches on.
#[tokio::test]
async fn a_file_that_is_no_image_and_an_unknown_id_are_refused_by_name() {
    let core = test_core().await;
    let (runs, _, _) = registry();
    let chats = HubChats::new(core, &Arc::new(runs));

    let refused = chats.attach(b"GIF89a").await.unwrap_err();
    assert!(matches!(refused, AttachmentError::Unsupported), "{refused}");

    let never = AttachmentId::of(b"never uploaded");
    let missing = chats.attachment(&never).await.unwrap_err();
    assert!(
        matches!(&missing, AttachmentError::NotFound(id) if *id == never),
        "{missing}"
    );
}

/// A picture a device keeps joins the tool row that carries the one it was
/// made from, last, and the chat opens with it there. A chat that is not
/// there, a source no tool row of the chat carries and a picture the store
/// lacks are each refused by name, and none of them changes the row.
#[tokio::test]
async fn a_devices_kept_picture_joins_its_sources_tool_row_and_is_read_back_last() {
    let core = test_core().await;
    let (runs, _, _) = registry();
    let chats = HubChats::new(Arc::clone(&core), &Arc::new(runs));
    let drawn = chats.attach(&png_of(64, 64)).await.unwrap().info;
    let fixed = chats.attach(&png_of(64, 32)).await.unwrap().info;
    let mine = chats.attach(&png_of(32, 32)).await.unwrap().info;
    let history = core.chat_history();
    let id = history
        .create_conversation(NewConversation {
            title: "t".to_owned(),
            ..NewConversation::default()
        })
        .await
        .unwrap();
    for (role, image) in [(MessageRole::User, &mine), (MessageRole::Tool, &drawn)] {
        let row = NewMessage {
            conversation_id: id,
            role,
            content: "row".to_owned(),
            metadata: None,
            images: vec![image.id.clone()],
        };
        history.save_message(row).await.unwrap();
    }

    let never = AttachmentId::of(b"never uploaded");
    let refusals = [
        (id + 1, &drawn.id, &fixed.id, HubChatsError::NotFound),
        // On a user row of the chat, and so on no tool row of it.
        (id, &mine.id, &fixed.id, HubChatsError::PictureNotFound),
        (id, &never, &fixed.id, HubChatsError::PictureNotFound),
        (id, &drawn.id, &never, HubChatsError::PictureNotFound),
    ];
    for (chat, from, image, want) in refusals {
        let refused = chats.keep_picture(chat, from, image).await;
        assert_eq!(refused, Err(want));
    }
    let open = chats.open(id).await.unwrap();
    assert_eq!(open.messages[1].images, vec![drawn.clone()]);

    let kept = chats.keep_picture(id, &drawn.id, &fixed.id).await.unwrap();
    assert_eq!(kept, vec![drawn.clone(), fixed.clone()]);

    let open = chats.open(id).await.unwrap();
    assert_eq!(open.messages[0].images, vec![mine]);
    assert_eq!(open.messages[1].images, vec![drawn, fixed]);
}
