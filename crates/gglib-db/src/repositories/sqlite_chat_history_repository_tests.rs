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
        images: Vec::new(),
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
async fn update_conversation_model_sets_and_clears_it_alone() {
    let repo = repo().await;
    sqlx::query(
        "INSERT INTO models (id, name, file_path, param_count_b, added_at, model_key)
         VALUES (7, 'm', '/tmp/m.gguf', 0.5, '2026-01-01 00:00:00', 'm')",
    )
    .execute(&repo.pool)
    .await
    .expect("seed model");
    let id = repo.create_conversation(make_conv("Kept")).await.unwrap();
    let set = |model_id| ConversationUpdate {
        model_id: Some(model_id),
        ..Default::default()
    };
    repo.update_conversation(id, set(Some(7))).await.unwrap();
    let conversation = repo.get_conversation(id).await.unwrap().unwrap();
    assert_eq!(
        (conversation.model_id, conversation.title.as_str()),
        (Some(7), "Kept")
    );
    repo.update_conversation(id, set(None)).await.unwrap();
    let conversation = repo.get_conversation(id).await.unwrap().unwrap();
    assert_eq!(conversation.model_id, None);
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

#[tokio::test]
async fn save_messages_writes_every_row_in_order() {
    let repo = repo().await;
    let id = repo.create_conversation(make_conv("t")).await.unwrap();

    let rows = vec![
        make_msg(id, "one"),
        make_msg(id, "two"),
        make_msg(id, "three"),
    ];
    repo.save_messages(rows).await.unwrap();

    let saved = repo.get_messages(id).await.unwrap();
    let contents: Vec<&str> = saved.iter().map(|m| m.content.as_str()).collect();
    assert_eq!(contents, ["one", "two", "three"]);
}

/// A row that cannot be written (no such conversation) takes the rows
/// before it with it.
#[tokio::test]
async fn save_messages_with_a_failing_row_writes_nothing() {
    let repo = repo().await;
    let id = repo.create_conversation(make_conv("t")).await.unwrap();

    let rows = vec![make_msg(id, "one"), make_msg(id + 1000, "orphan")];
    assert!(repo.save_messages(rows).await.is_err());

    assert!(repo.get_messages(id).await.unwrap().is_empty());
}

/// Cut off after any number of steps, the call has written all of its rows
/// or none of them.
#[tokio::test]
async fn save_messages_cut_off_part_way_writes_all_or_nothing() {
    use std::task::{Context, Poll, Waker};

    let repo = repo().await;
    for steps in 0..40 {
        let id = repo.create_conversation(make_conv("t")).await.unwrap();
        let rows: Vec<NewMessage> = (0..5).map(|n| make_msg(id, &n.to_string())).collect();
        let mut save = Box::pin(repo.save_messages(rows));
        for _ in 0..steps {
            let poll = save.as_mut().poll(&mut Context::from_waker(Waker::noop()));
            if matches!(poll, Poll::Ready(_)) {
                break;
            }
            tokio::task::yield_now().await;
        }
        drop(save);
        let written = repo.get_messages(id).await.unwrap().len();
        assert!(
            written == 0 || written == 5,
            "{written} rows after {steps} steps"
        );
    }
}

#[tokio::test]
async fn replace_from_deletes_the_tail_and_saves_the_new_message() {
    let repo = repo().await;
    let cid = repo.create_conversation(make_conv("Edit")).await.unwrap();
    repo.save_message(make_msg(cid, "A")).await.unwrap();
    let b = repo.save_message(make_msg(cid, "B")).await.unwrap();
    repo.save_message(make_msg(cid, "C")).await.unwrap();

    let id = repo.replace_from(b, make_msg(cid, "B2")).await.unwrap();

    let rows = repo.get_messages(cid).await.unwrap();
    let contents: Vec<_> = rows.iter().map(|m| m.content.as_str()).collect();
    assert_eq!(contents, ["A", "B2"]);
    assert_eq!(rows[1].id, id);
}

#[tokio::test]
async fn replace_from_a_row_of_another_conversation_changes_nothing() {
    let repo = repo().await;
    let mine = repo.create_conversation(make_conv("Mine")).await.unwrap();
    let theirs = repo.create_conversation(make_conv("Theirs")).await.unwrap();
    // Theirs is older than every row of mine, so a delete from its id on
    // that ignored whose row it is would take all of mine.
    let other = repo.save_message(make_msg(theirs, "X")).await.unwrap();
    repo.save_message(make_msg(mine, "A")).await.unwrap();

    let refused = repo.replace_from(other, make_msg(mine, "B")).await;

    assert!(matches!(refused, Err(ChatHistoryError::MessageNotFound(id)) if id == other));
    assert_eq!(repo.get_messages(mine).await.unwrap().len(), 1);
    assert_eq!(repo.get_messages(theirs).await.unwrap().len(), 1);
}

#[tokio::test]
async fn replace_from_that_cannot_save_deletes_nothing() {
    let repo = repo().await;
    let cid = repo
        .create_conversation(make_conv("Rollback"))
        .await
        .unwrap();
    let a = repo.save_message(make_msg(cid, "A")).await.unwrap();
    repo.save_message(make_msg(cid, "B")).await.unwrap();
    sqlx::query(
        "CREATE TRIGGER refuse BEFORE INSERT ON chat_messages BEGIN SELECT RAISE(ABORT, 'no'); END",
    )
    .execute(&repo.pool)
    .await
    .unwrap();

    assert!(repo.replace_from(a, make_msg(cid, "A2")).await.is_err());

    assert_eq!(repo.get_messages(cid).await.unwrap().len(), 2);
}
