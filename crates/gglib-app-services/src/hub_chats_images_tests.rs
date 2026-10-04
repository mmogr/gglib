//! An image a paired device sends is stored by the hub's one ingest, shown
//! on the row that names it without its bytes, and read back by its id.

use std::sync::Arc;

use gglib_core::domain::AttachmentId;
use gglib_core::domain::chat::{MessageRole, NewMessage};
use gglib_core::ports::{AttachmentError, HubChatsPort};
use gglib_core::request_pipeline::estimate_image_tokens;

use super::HubChats;
use crate::runs::test_executor::registry;
use crate::test_support::test_core;

/// A PNG's signature and `IHDR`, 640 by 480: all the store reads of one.
fn png() -> Vec<u8> {
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    bytes.extend([0, 0, 0, 13]);
    bytes.extend(b"IHDR");
    bytes.extend(640_u32.to_be_bytes());
    bytes.extend(480_u32.to_be_bytes());
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
        .create_conversation("t".to_owned(), None, None)
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
