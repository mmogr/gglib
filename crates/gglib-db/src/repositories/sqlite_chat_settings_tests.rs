//! Every writer of a conversation's settings changes only the keys it means
//! to: a key this build does not know, and a value it cannot read, stay as
//! they are stored. Text that is not a JSON object is
//! `sqlite_chat_settings_text_tests`'.

use std::sync::Arc;

use gglib_core::domain::chat::{ConversationSettings, NewConversation};
use gglib_core::domain::conversation_patch::ConversationSettingsPatch;
use gglib_core::domain::{Machine, ModelRef, Thinking};
use gglib_core::ports::chat_history::ChatHistoryRepository;
use gglib_core::services::ChatHistoryService;

use super::SqliteChatHistoryRepository;
use crate::setup::setup_test_database;

/// Settings a newer build might store: a temperature this build cannot read,
/// a key it does not know, and two it reads.
pub(super) const STORED: &str = r#"{"model_name":"Qwen","temperature":"warm","later":{"a":[1,2.50]},"thinking":"off","max_parallel":4}"#;

pub(super) struct Chat {
    pub(super) repo: Arc<SqliteChatHistoryRepository>,
    pub(super) service: ChatHistoryService,
    pub(super) id: i64,
}

/// A chat whose settings column holds `text`, and model 7 in the catalogue.
pub(super) async fn chat(text: Option<&str>) -> Chat {
    let repo = Arc::new(SqliteChatHistoryRepository::new(
        setup_test_database().await.unwrap(),
    ));
    sqlx::query(
        "INSERT INTO models (id, name, file_path, param_count_b, added_at, model_key) \
         VALUES (7, 'm', '/tmp/m.gguf', 0.5, '2026-01-01 00:00:00', 'm')",
    )
    .execute(&repo.pool)
    .await
    .unwrap();
    let new = NewConversation {
        title: "Kept".to_owned(),
        ..NewConversation::default()
    };
    let id = repo.create_conversation(new).await.unwrap();
    sqlx::query("UPDATE chat_conversations SET settings = ? WHERE id = ?")
        .bind(text)
        .bind(id)
        .execute(&repo.pool)
        .await
        .unwrap();
    let service = ChatHistoryService::new(Arc::clone(&repo) as Arc<dyn ChatHistoryRepository>);
    Chat { repo, service, id }
}

impl Chat {
    /// The settings column and `model_id`, as stored.
    pub(super) async fn stored(&self) -> (Option<String>, Option<i64>) {
        sqlx::query_as("SELECT settings, model_id FROM chat_conversations WHERE id = ?")
            .bind(self.id)
            .fetch_one(&self.repo.pool)
            .await
            .unwrap()
    }

    async fn held(&self) -> ConversationSettings {
        let conversation = self.service.get_conversation(self.id).await.unwrap();
        conversation.unwrap().settings.unwrap_or_default()
    }
}

pub(super) fn local(id: i64) -> ModelRef {
    ModelRef {
        machine: Machine::Local,
        id,
    }
}

#[tokio::test]
async fn a_key_that_cannot_be_read_is_passed_over_and_the_rest_are_read() {
    let chat = chat(Some(STORED)).await;
    let held = chat.held().await;
    assert_eq!(held.model_name.as_deref(), Some("Qwen"));
    assert_eq!(
        (held.thinking, held.max_parallel),
        (Some(Thinking::Off), Some(4))
    );
    assert_eq!(held.temperature, None);
}

#[tokio::test]
async fn thinking_is_forgotten_and_every_other_key_kept() {
    let chat = chat(Some(STORED)).await;
    chat.service.record_thinking(chat.id, None).await.unwrap();
    let expected =
        r#"{"model_name":"Qwen","temperature":"warm","later":{"a":[1,2.50]},"max_parallel":4}"#;
    assert_eq!(chat.stored().await, (Some(expected.to_owned()), None));
}

#[tokio::test]
async fn a_folder_is_added_and_every_other_key_kept() {
    let chat = chat(Some(STORED)).await;
    let folder = Some("/work".to_owned());
    chat.service.record_folder(chat.id, folder).await.unwrap();
    let expected = r#"{"model_name":"Qwen","temperature":"warm","later":{"a":[1,2.50]},"thinking":"off","max_parallel":4,"folder":"/work"}"#;
    assert_eq!(chat.stored().await, (Some(expected.to_owned()), None));
}

#[tokio::test]
async fn a_run_names_its_model_and_every_other_key_is_kept() {
    let chat = chat(Some(STORED)).await;
    let model = Some(local(7));
    chat.service
        .record_model(chat.id, model, "Llama")
        .await
        .unwrap();
    let expected = r#"{"model_name":"Llama","temperature":"warm","later":{"a":[1,2.50]},"thinking":"off","max_parallel":4,"model":{"id":7,"machine":{"kind":"local"}}}"#;
    assert_eq!(chat.stored().await, (Some(expected.to_owned()), Some(7)));
}

#[tokio::test]
async fn a_patch_changes_its_settings_and_every_other_key_is_kept() {
    let chat = chat(Some(STORED)).await;
    let patch = ConversationSettingsPatch {
        tools: Some(vec!["read_file".to_owned()]),
        max_parallel: Some(None),
        ..ConversationSettingsPatch::default()
    };
    let title = Some("Renamed".to_owned());
    let changed = chat
        .service
        .update_conversation(chat.id, title, None, patch, None);
    changed.await.unwrap();
    let expected = r#"{"model_name":"Qwen","temperature":"warm","later":{"a":[1,2.50]},"thinking":"off","tools":["read_file"]}"#;
    assert_eq!(chat.stored().await, (Some(expected.to_owned()), None));
}

/// A resume's command line names a temperature, over the one this build
/// cannot read, and a top-k.
#[tokio::test]
async fn a_resume_writes_what_it_names_and_every_other_key_is_kept() {
    let chat = chat(Some(STORED)).await;
    let held = chat.held().await;
    let settings = ConversationSettings {
        temperature: Some(0.7),
        top_k: Some(40),
        ..held.clone()
    };
    chat.service
        .record_settings(chat.id, &held, settings)
        .await
        .unwrap();
    let expected = r#"{"model_name":"Qwen","temperature":0.7,"later":{"a":[1,2.50]},"thinking":"off","max_parallel":4,"top_k":40}"#;
    assert_eq!(chat.stored().await, (Some(expected.to_owned()), None));
}

/// A model on another machine is replaced whole: nothing of the paired one,
/// its fingerprint, stays in the new one.
#[tokio::test]
async fn a_new_model_replaces_the_old_one_whole() {
    let chat = chat(Some(
        r#"{"model":{"machine":{"kind":"paired","fingerprint":"ab"},"id":3}}"#,
    ))
    .await;
    let model = Some(local(7));
    chat.service
        .record_model(chat.id, model, "Llama")
        .await
        .unwrap();
    let expected = r#"{"model":{"id":7,"machine":{"kind":"local"}},"model_name":"Llama"}"#;
    assert_eq!(chat.stored().await, (Some(expected.to_owned()), Some(7)));
}

/// Clearing a value this build cannot read keeps it: it already reads as
/// none.
#[tokio::test]
async fn clearing_a_value_that_cannot_be_read_keeps_it() {
    let chat = chat(Some(r#"{"max_parallel":"many","model_name":"Qwen"}"#)).await;
    let patch = ConversationSettingsPatch {
        max_parallel: Some(None),
        ..ConversationSettingsPatch::default()
    };
    let changed = chat
        .service
        .update_conversation(chat.id, None, None, patch, None);
    changed.await.unwrap();
    let expected = r#"{"max_parallel":"many","model_name":"Qwen"}"#;
    assert_eq!(chat.stored().await, (Some(expected.to_owned()), None));
}

/// The model is written with the `model_id` it decides, even one this build
/// cannot read: a run on a model the registry lacks names none, and the
/// conversation then has no `model_id`.
#[tokio::test]
async fn a_model_is_written_with_its_model_id() {
    let chat = chat(Some(r#"{"model":"newer","model_name":"Qwen"}"#)).await;
    chat.service
        .record_model(chat.id, None, "Qwen 2")
        .await
        .unwrap();
    let expected = r#"{"model_name":"Qwen 2"}"#;
    assert_eq!(chat.stored().await, (Some(expected.to_owned()), None));
}

/// A writer whose read is out of date keeps what was written since it read,
/// and writes the model it holds, and its name, beside the `model_id` that
/// names it, so the three never disagree.
#[tokio::test]
async fn a_writer_with_an_old_read_keeps_what_others_wrote_since() {
    let chat = chat(None).await;
    chat.service
        .record_model(chat.id, Some(local(7)), "Llama")
        .await
        .unwrap();
    let held = chat.held().await;
    let paired = ModelRef {
        machine: Machine::Paired {
            fingerprint: "ab".to_owned(),
        },
        id: 3,
    };
    chat.service
        .record_model(chat.id, Some(paired), "Far")
        .await
        .unwrap();
    let folder = Some("/work".to_owned());
    chat.service.record_folder(chat.id, folder).await.unwrap();
    let settings = ConversationSettings {
        top_k: Some(40),
        ..held.clone()
    };
    chat.service
        .record_settings(chat.id, &held, settings)
        .await
        .unwrap();
    let expected = r#"{"model":{"id":7,"machine":{"kind":"local"}},"model_name":"Llama","folder":"/work","top_k":40}"#;
    assert_eq!(chat.stored().await, (Some(expected.to_owned()), Some(7)));
}
