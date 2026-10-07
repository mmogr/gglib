//! A run's user message is saved with the images it names, or refused by
//! code when it names one that was never uploaded.

use axum::http::StatusCode;
use gglib_core::domain::AttachmentId;
use gglib_core::domain::agent::AgentMessage;
use gglib_core::domain::chat::NewConversation;

use super::run_fixture::state;
use super::transcript::save_user;
use crate::error::HttpError;

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

fn user_with(images: Vec<AttachmentId>) -> AgentMessage {
    AgentMessage::User {
        content: "what is the error?".to_owned(),
        images,
    }
}

#[tokio::test]
async fn a_user_message_is_saved_with_the_images_it_names() {
    let (_dir, state) = state().await;
    let chats = state.core.chat_history();
    let conversation = chats
        .create_conversation(NewConversation {
            title: "c".to_owned(),
            ..NewConversation::default()
        })
        .await
        .unwrap();
    let image = state.core.attachments().ingest(&png()).await.unwrap().info;

    let message = user_with(vec![image.id.clone()]);
    save_user(&state.core, conversation, None, Some(&message), None)
        .await
        .unwrap();

    let saved = chats.get_messages(conversation).await.unwrap();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].images, vec![image]);
}

#[tokio::test]
async fn a_user_message_naming_an_unknown_image_is_refused_by_code_and_not_saved() {
    let (_dir, state) = state().await;
    let chats = state.core.chat_history();
    let conversation = chats
        .create_conversation(NewConversation {
            title: "c".to_owned(),
            ..NewConversation::default()
        })
        .await
        .unwrap();
    let missing = AttachmentId::of(b"never uploaded");

    let message = user_with(vec![missing.clone()]);
    let refusal = save_user(&state.core, conversation, None, Some(&message), None)
        .await
        .unwrap_err();

    let HttpError::Coded {
        status,
        code,
        message,
    } = refusal
    else {
        panic!("a coded refusal, not {refusal:?}");
    };
    assert_eq!(
        (status, code),
        (StatusCode::BAD_REQUEST, "attachment_not_found")
    );
    assert!(message.contains(missing.as_str()), "{message}");
    assert!(chats.get_messages(conversation).await.unwrap().is_empty());
}
