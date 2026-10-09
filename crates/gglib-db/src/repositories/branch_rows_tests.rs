//! A branch copied from a chat, and a family read back (ADR 0017).

use gglib_core::domain::attachment::AttachmentId;
use gglib_core::domain::chat::{MessageRole, NewMessage};
use gglib_core::ports::chat_history::{ChatHistoryError, ChatHistoryRepository};

use crate::repositories::chat_fixture::{KYOTO, chats, message};

use MessageRole::{System, User};

#[tokio::test]
async fn a_branch_copies_the_chat_as_far_as_a_message_and_leaves_it_as_it_was() {
    let chats = chats().await;
    let (source, ids) = chats.chat("Kyoto", &KYOTO).await;
    let image = chats.image("a map").await;
    let then = NewMessage {
        images: vec![image.clone()],
        ..message(source, User, "Make it shorter")
    };

    let branch = chats
        .repo
        .fork(source, Some(ids[1]), Some(then))
        .await
        .unwrap();

    assert_eq!(
        chats.contents(branch).await,
        ["Plan a trip to Kyoto", "Day 1: temples", "Make it shorter"]
    );
    assert_eq!(chats.contents(source).await.len(), 4);
    let copied = chats.repo.get_messages(branch).await.unwrap();
    let originals = chats.repo.get_messages(source).await.unwrap();
    assert_eq!(copied[0].key(), ids[0]);
    assert_eq!(copied[1].key(), ids[1]);
    assert_eq!(copied[1].created_at, originals[1].created_at);
    assert_eq!(copied[2].key(), copied[2].id, "the edit is written here");
    assert_eq!(copied[2].images[0].id, image);
    let made = chats.repo.get_conversation(branch).await.unwrap().unwrap();
    assert_eq!(made.title, "Kyoto");
    assert_eq!(made.system_prompt.as_deref(), Some("Be brief."));
    assert_eq!(made.settings.and_then(|s| s.temperature), Some(0.2));
    assert_eq!(
        (made.branch_of, made.lineage_id),
        (Some(source), Some(source))
    );
}

#[tokio::test]
async fn a_branch_of_a_branch_is_of_the_same_family_and_remembers_the_first_message() {
    let chats = chats().await;
    let (source, ids) = chats.chat("Kyoto", &KYOTO).await;
    let first = chats.repo.fork(source, Some(ids[3]), None).await.unwrap();
    let firsts = chats.repo.get_messages(first).await.unwrap();

    let second = chats
        .repo
        .fork(first, Some(firsts[1].id), None)
        .await
        .unwrap();

    let seconds = chats.repo.get_messages(second).await.unwrap();
    assert_eq!(seconds[0].key(), ids[0]);
    assert_eq!(seconds[1].key(), ids[1]);
    let made = chats.repo.get_conversation(second).await.unwrap().unwrap();
    assert_eq!(
        (made.branch_of, made.lineage_id),
        (Some(first), Some(source))
    );
}

/// A chat's prompt is a setting of the chat, and a saved system row is
/// never sent: a branch copies none.
#[tokio::test]
async fn a_branch_copies_no_system_message_and_nothing_with_no_message_named() {
    let chats = chats().await;
    let (source, ids) = chats.chat("Kyoto", &[(System, "S"), (User, "Q")]).await;
    let all = chats.repo.fork(source, Some(ids[1]), None).await.unwrap();
    let none = chats.repo.fork(source, None, None).await.unwrap();
    assert_eq!(chats.contents(all).await, ["Q"]);
    assert!(chats.contents(none).await.is_empty());
}

#[tokio::test]
async fn a_branch_that_cannot_be_made_writes_nothing() {
    let chats = chats().await;
    let (source, _) = chats.chat("Kyoto", &KYOTO).await;
    let (_, others) = chats.chat("Osaka", &KYOTO[..1]).await;
    let (conversations, messages) = (
        chats.count("chat_conversations").await,
        chats.count("chat_messages").await,
    );

    let elsewhere = chats.repo.fork(source, Some(others[0]), None).await;
    let nowhere = chats.repo.fork(99, None, None).await;
    let unstored = NewMessage {
        images: vec![AttachmentId::of(b"never stored")],
        ..message(source, User, "Q")
    };
    let no_image = chats.repo.fork(source, None, Some(unstored)).await;

    assert!(matches!(elsewhere, Err(ChatHistoryError::MessageNotFound(id)) if id == others[0]));
    assert!(matches!(
        nowhere,
        Err(ChatHistoryError::ConversationNotFound(99))
    ));
    assert!(matches!(no_image, Err(ChatHistoryError::Attachment(_))));
    assert_eq!(chats.count("chat_conversations").await, conversations);
    assert_eq!(chats.count("chat_messages").await, messages);
}

#[tokio::test]
async fn a_family_is_read_from_any_of_its_chats_and_outlives_the_first() {
    let chats = chats().await;
    let (source, ids) = chats.chat("Kyoto", &KYOTO).await;
    let (_stranger, _) = chats.chat("Osaka", &KYOTO).await;
    let branch = chats.repo.fork(source, Some(ids[1]), None).await.unwrap();

    let family = chats.repo.lineage(branch).await.unwrap();
    let members: Vec<i64> = family.iter().map(|chat| chat.conversation_id).collect();
    assert_eq!(members, [source, branch]);
    assert_eq!(family[0].rows.len(), 4);
    assert_eq!(family[1].rows[1].key, ids[1]);

    chats.repo.delete_conversation(source).await.unwrap();
    let family = chats.repo.lineage(branch).await.unwrap();
    assert_eq!(family.len(), 1);
    assert_eq!(family[0].conversation_id, branch);
}
