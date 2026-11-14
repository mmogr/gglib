//! Settings stored as text that is not a JSON object, or never stored: a
//! change to them is refused and the text left as it is, or written with
//! only what changes. Beside `sqlite_chat_settings_tests`, whose chats these
//! use.

use gglib_core::domain::Thinking;
use gglib_core::domain::chat::{ConversationSettings, ConversationUpdate};
use gglib_core::domain::conversation_patch::ConversationSettingsPatch;
use gglib_core::ports::chat_history::{ChatHistoryError, ChatHistoryRepository};

use super::sqlite_chat_settings_tests::{STORED, chat, local};

#[tokio::test]
async fn text_that_is_not_an_object_is_left_alone_by_every_writer() {
    for text in ["not json", "[1]", "null"] {
        let chat = chat(Some(text)).await;
        let (service, id) = (&chat.service, chat.id);
        let tools = ConversationSettingsPatch {
            tools: Some(vec!["read_file".to_owned()]),
            ..ConversationSettingsPatch::default()
        };
        let resumed = ConversationSettings {
            max_iterations: Some(40),
            ..ConversationSettings::default()
        };
        let default = ConversationSettings::default();
        let refusals = [
            service.record_thinking(id, Some(Thinking::Off)).await,
            service.record_folder(id, Some("/work".to_owned())).await,
            service.record_model(id, Some(local(7)), "Llama").await,
            service
                .update_conversation(id, Some("New".to_owned()), None, tools, None)
                .await,
            service.record_settings(id, &default, resumed).await,
        ];
        for refusal in refusals {
            assert!(
                matches!(refusal, Err(ChatHistoryError::UnreadableSettings(i)) if i == id),
                "{text}: {refusal:?}"
            );
        }
        assert_eq!(chat.stored().await, (Some(text.to_owned()), None), "{text}");
        let conversation = service.get_conversation(id).await.unwrap().unwrap();
        assert_eq!(conversation.title, "Kept", "{text}");
    }
}

/// A change that names no setting is made, and the text stays.
#[tokio::test]
async fn a_title_is_changed_beside_text_that_is_not_an_object() {
    let chat = chat(Some("not json")).await;
    let patch = ConversationSettingsPatch::default();
    let title = Some("New".to_owned());
    let changed = chat
        .service
        .update_conversation(chat.id, title, None, patch, None);
    changed.await.unwrap();
    let conversation = chat
        .service
        .get_conversation(chat.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(conversation.title, "New");
    assert_eq!(chat.stored().await, (Some("not json".to_owned()), None));
}

#[tokio::test]
async fn settings_never_stored_are_written_with_what_changes_or_left_unset() {
    let chat = chat(None).await;
    chat.service.record_thinking(chat.id, None).await.unwrap();
    let removed = [("thinking".to_owned(), None)].into();
    let update = ConversationUpdate {
        settings: removed,
        ..ConversationUpdate::default()
    };
    chat.repo
        .update_conversation(chat.id, update)
        .await
        .unwrap();
    assert_eq!(chat.stored().await, (None, None));
    let folder = Some("/work".to_owned());
    chat.service.record_folder(chat.id, folder).await.unwrap();
    let expected = r#"{"folder":"/work"}"#;
    assert_eq!(chat.stored().await, (Some(expected.to_owned()), None));
}

#[tokio::test]
async fn a_branch_copies_the_settings_text_as_stored() {
    let chat = chat(Some(STORED)).await;
    let branch = chat.repo.fork(chat.id, None, None).await.unwrap();
    let copied: Option<String> =
        sqlx::query_scalar("SELECT settings FROM chat_conversations WHERE id = ?")
            .bind(branch)
            .fetch_one(&chat.repo.pool)
            .await
            .unwrap();
    assert_eq!(copied.as_deref(), Some(STORED));
}
