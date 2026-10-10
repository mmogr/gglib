//! The chat history service's changes over `SQLite`: an edit, a regenerate
//! and a branch, and the branch points they leave (ADR 0017).

use serde_json::json;

use gglib_core::domain::branching::{ChatChange, ChatChanged, EDITED_KEY};
use gglib_core::domain::chat::MessageRole;
use gglib_core::ports::chat_history::{ChatHistoryError, ChatHistoryRepository};
use gglib_core::services::ChangeError;

use crate::repositories::chat_fixture::{KYOTO, chats};

use MessageRole::{Assistant, Tool, User};

fn edit(message_id: i64, content: &str) -> ChatChange {
    ChatChange::Edit {
        message_id,
        content: content.to_owned(),
        images: Vec::new(),
    }
}

#[tokio::test]
async fn an_edit_of_an_answered_question_is_a_branch_to_answer_and_both_chats_offer_both() {
    let chats = chats().await;
    let service = chats.service();
    let (source, ids) = chats.chat("Kyoto", &KYOTO).await;

    let changed = service
        .change(source, &edit(ids[2], "Make it shorter"), false)
        .await
        .unwrap();

    assert!(changed.forked && changed.answer);
    let branch = changed.conversation_id;
    assert_eq!(
        chats.contents(branch).await,
        ["Plan a trip to Kyoto", "Day 1: temples", "Make it shorter"]
    );
    assert_eq!(chats.contents(source).await.len(), 4);
    let from_the_branch = service.thread(branch).await.unwrap();
    assert!(from_the_branch.answerable);
    let point = &from_the_branch.points[0];
    assert_eq!(point.index, 1);
    let options: Vec<(i64, &str)> = point
        .options
        .iter()
        .map(|o| (o.conversation_id, o.preview.as_str()))
        .collect();
    assert_eq!(
        options,
        [(source, "Make it cheaper"), (branch, "Make it shorter")]
    );
    let from_the_source = service.thread(source).await.unwrap();
    assert_eq!(from_the_source.points[0].index, 0);
    assert!(!from_the_source.answerable);
}

#[tokio::test]
async fn an_edit_of_the_last_question_nothing_answers_is_made_in_place() {
    let chats = chats().await;
    let service = chats.service();
    let (source, ids) = chats.chat("Kyoto", &KYOTO[..3]).await;

    let changed = service
        .change(source, &edit(ids[2], "Make it shorter"), false)
        .await
        .unwrap();

    assert_eq!(
        changed,
        ChatChanged {
            conversation_id: source,
            forked: false,
            answer: true
        }
    );
    assert_eq!(
        chats.contents(source).await,
        ["Plan a trip to Kyoto", "Day 1: temples", "Make it shorter"]
    );
    assert_eq!(chats.count("chat_conversations").await, 1);
}

#[tokio::test]
async fn an_edit_of_a_question_being_answered_is_a_branch() {
    let chats = chats().await;
    let (source, ids) = chats.chat("Kyoto", &KYOTO[..3]).await;
    let changed = chats
        .service()
        .change(source, &edit(ids[2], "Make it shorter"), true)
        .await
        .unwrap();
    assert!(changed.forked);
    assert_eq!(chats.contents(source).await[2], "Make it cheaper");
}

/// The reply's tool call and its result go with it: the branch holds the
/// edited text alone, marked as the person's.
#[tokio::test]
async fn an_edited_reply_is_a_branch_holding_one_message_of_plain_text() {
    let chats = chats().await;
    let (source, ids) = chats
        .chat(
            "Kyoto",
            &[(User, "Q"), (Assistant, ""), (Tool, "{}"), (Assistant, "A")],
        )
        .await;
    let changed = chats
        .service()
        .change(source, &edit(ids[3], "B"), false)
        .await
        .unwrap();

    assert!(changed.forked && !changed.answer);
    let rows = chats
        .repo
        .get_messages(changed.conversation_id)
        .await
        .unwrap();
    let shown: Vec<(MessageRole, &str)> =
        rows.iter().map(|r| (r.role, r.content.as_str())).collect();
    assert_eq!(shown, [(User, "Q"), (Assistant, "B")]);
    assert_eq!(rows[1].metadata, Some(json!({ EDITED_KEY: true })));
}

#[tokio::test]
async fn a_regenerate_is_a_branch_ending_in_its_question_to_answer() {
    let chats = chats().await;
    let (source, ids) = chats.chat("Kyoto", &KYOTO).await;
    let changed = chats
        .service()
        .change(
            source,
            &ChatChange::Regenerate { message_id: ids[3] },
            false,
        )
        .await
        .unwrap();
    assert!(changed.forked && changed.answer);
    assert_eq!(
        chats.contents(changed.conversation_id).await,
        KYOTO[..3].iter().map(|(_, c)| *c).collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn a_change_to_no_chat_or_a_refused_change_writes_nothing() {
    let chats = chats().await;
    let service = chats.service();
    let (source, ids) = chats.chat("Kyoto", &KYOTO).await;

    let nowhere = service
        .change(99, &ChatChange::Branch { message_id: ids[0] }, false)
        .await;
    let refused = service
        .change(
            source,
            &ChatChange::Regenerate { message_id: ids[0] },
            false,
        )
        .await;

    assert!(matches!(
        nowhere,
        Err(ChangeError::History(
            ChatHistoryError::ConversationNotFound(99)
        ))
    ));
    assert!(matches!(refused, Err(ChangeError::Refused(_))));
    assert_eq!(chats.count("chat_conversations").await, 1);
}
