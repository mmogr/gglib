//! The `SQLite` chat history repository against a test database.

use gglib_core::domain::chat::{ConversationUpdate, MessageRole, NewConversation, NewMessage};
use gglib_core::ports::chat_history::ChatHistoryRepository;

use crate::setup::setup_test_database;

use super::*;

// ── Helpers ───────────────────────────────────────────────────────────────

fn make_conv(title: &str) -> NewConversation {
    NewConversation {
        title: title.to_string(),
        model_id: None,
        system_prompt: None,
        settings: None,
    }
}

fn make_msg(conversation_id: i64, content: &str) -> NewMessage {
    NewMessage {
        conversation_id,
        role: MessageRole::User,
        content: content.to_string(),
        metadata: None,
    }
}

async fn repo() -> SqliteChatHistoryRepository {
    let pool = setup_test_database().await.expect("setup_test_database");
    SqliteChatHistoryRepository::new(pool)
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn create_and_list_conversations() {
    let repo = repo().await;
    repo.create_conversation(make_conv("Chat 1")).await.unwrap();
    assert_eq!(repo.list_conversations().await.unwrap().len(), 1);
}

#[tokio::test]
async fn get_conversation_by_id() {
    let repo = repo().await;
    let id = repo
        .create_conversation(make_conv("Explore"))
        .await
        .unwrap();
    let conv = repo.get_conversation(id).await.unwrap();
    assert_eq!(conv.unwrap().title, "Explore");
}

#[tokio::test]
async fn get_conversation_count() {
    let repo = repo().await;
    repo.create_conversation(make_conv("A")).await.unwrap();
    repo.create_conversation(make_conv("B")).await.unwrap();
    assert_eq!(repo.get_conversation_count().await.unwrap(), 2);
}

#[tokio::test]
async fn update_conversation_title() {
    let repo = repo().await;
    let id = repo.create_conversation(make_conv("Old")).await.unwrap();
    let update = ConversationUpdate {
        title: Some("New".to_string()),
        ..Default::default()
    };
    repo.update_conversation(id, update).await.unwrap();
    assert_eq!(
        repo.get_conversation(id).await.unwrap().unwrap().title,
        "New"
    );
}

#[tokio::test]
async fn delete_conversation() {
    let repo = repo().await;
    let id = repo.create_conversation(make_conv("Tmp")).await.unwrap();
    repo.delete_conversation(id).await.unwrap();
    assert!(repo.list_conversations().await.unwrap().is_empty());
}

#[tokio::test]
async fn save_and_get_messages_round_trip() {
    let repo = repo().await;
    let cid = repo.create_conversation(make_conv("Msgs")).await.unwrap();
    repo.save_message(make_msg(cid, "Hello")).await.unwrap();
    repo.save_message(make_msg(cid, "World")).await.unwrap();
    assert_eq!(repo.get_messages(cid).await.unwrap().len(), 2);
}

#[tokio::test]
async fn get_message_count() {
    let repo = repo().await;
    let cid = repo.create_conversation(make_conv("Count")).await.unwrap();
    for i in 0..3 {
        repo.save_message(make_msg(cid, &format!("m{i}")))
            .await
            .unwrap();
    }
    assert_eq!(repo.get_message_count(cid).await.unwrap(), 3);
}

#[tokio::test]
async fn update_message_content() {
    let repo = repo().await;
    let cid = repo.create_conversation(make_conv("Edit")).await.unwrap();
    let mid = repo.save_message(make_msg(cid, "original")).await.unwrap();
    repo.update_message(mid, "updated".to_string(), None)
        .await
        .unwrap();
    assert_eq!(repo.get_messages(cid).await.unwrap()[0].content, "updated");
}

#[tokio::test]
async fn delete_message_and_subsequent_removes_tail() {
    let repo = repo().await;
    let cid = repo.create_conversation(make_conv("Tail")).await.unwrap();
    repo.save_message(make_msg(cid, "A")).await.unwrap();
    let mid_b = repo.save_message(make_msg(cid, "B")).await.unwrap();
    repo.save_message(make_msg(cid, "C")).await.unwrap();
    let removed = repo.delete_message_and_subsequent(mid_b).await.unwrap();
    assert_eq!(removed, 2);
    assert_eq!(repo.get_messages(cid).await.unwrap().len(), 1);
}
