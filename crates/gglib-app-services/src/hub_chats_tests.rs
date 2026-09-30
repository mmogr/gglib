//! The hub's chats as a paired device reads them: newest first, each with
//! its model's name and its live run, and one opened with its rows.

use std::sync::Arc;

use gglib_core::domain::chat::{MessageRole, NewMessage};
use gglib_core::domain::runs::{RunError, RunKind};
use gglib_core::ports::{HubChatsError, HubChatsPort};
use serde_json::json;

use super::HubChats;
use crate::runs::test_executor::registry;
use crate::runs::{Reservation, RunSpec};
use crate::test_support::test_core;

#[tokio::test]
async fn the_list_is_newest_first_with_each_chats_live_run() {
    let core = test_core().await;
    let (runs, _, _) = registry();
    let runs = Arc::new(runs);
    let history = core.chat_history();
    let older = history
        .create_conversation("older".to_owned(), None, None)
        .await
        .unwrap();
    // A second apart: `updated_at` has one-second resolution.
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    let newer = history
        .create_conversation("newer".to_owned(), None, None)
        .await
        .unwrap();
    let spec = RunSpec {
        kind: RunKind::Agent,
        model: None,
        conversation_id: Some(older),
    };
    let Ok(Reservation::New(reserved)) = runs.reserve("a1", spec) else {
        panic!("a new reservation");
    };
    reserved.start(
        |_| Box::pin(std::future::pending::<Result<(), RunError>>()),
        Box::new(|_, _| Box::pin(async { Ok(()) })),
    );

    let chats = HubChats::new(Arc::clone(&core), &runs)
        .list()
        .await
        .unwrap();
    let seen: Vec<(i64, &str, Option<&str>)> = chats
        .chats
        .iter()
        .map(|c| (c.id, c.title.as_str(), c.live_run.as_deref()))
        .collect();
    assert_eq!(
        seen,
        vec![(newer, "newer", None), (older, "older", Some("a1"))]
    );
    runs.shutdown();
}

#[tokio::test]
async fn a_chat_names_its_model_when_the_catalogue_has_it() {
    let core = test_core().await;
    let (runs, _, _) = registry();
    let model = gglib_core::domain::NewModel::new(
        "qwen3-8b".to_owned(),
        std::path::PathBuf::from("/models/qwen.gguf"),
        8.0,
        chrono::Utc::now(),
    );
    let model_id = core.models().add(model).await.unwrap().id;
    core.chat_history()
        .create_conversation("t".to_owned(), Some(model_id), None)
        .await
        .unwrap();
    let chats = HubChats::new(Arc::clone(&core), &Arc::new(runs))
        .list()
        .await
        .unwrap();
    assert_eq!(chats.chats[0].model_id, Some(model_id));
    assert_eq!(chats.chats[0].model.as_deref(), Some("qwen3-8b"));
}

#[tokio::test]
async fn a_chat_opens_with_its_rows_and_their_metadata() {
    let core = test_core().await;
    let (runs, _, _) = registry();
    let history = core.chat_history();
    let id = history
        .create_conversation("t".to_owned(), None, Some("be brief".to_owned()))
        .await
        .unwrap();
    for (role, content, metadata) in [
        (MessageRole::User, "hi", None),
        (
            MessageRole::Assistant,
            "hello",
            Some(json!({ "modelName": "qwen3-8b" })),
        ),
    ] {
        history
            .save_message(NewMessage {
                conversation_id: id,
                role,
                content: content.to_owned(),
                metadata,
            })
            .await
            .unwrap();
    }
    let chats = HubChats::new(Arc::clone(&core), &Arc::new(runs));
    let open = chats.open(id).await.unwrap();
    assert_eq!(open.conversation.system_prompt.as_deref(), Some("be brief"));
    let rows: Vec<(&str, Option<&serde_json::Value>)> = open
        .messages
        .iter()
        .map(|m| (m.content.as_str(), m.metadata.as_ref()))
        .collect();
    assert_eq!(
        rows,
        vec![
            ("hi", None),
            ("hello", Some(&json!({ "modelName": "qwen3-8b" })))
        ]
    );
    assert_eq!(
        chats.open(id + 1).await.err(),
        Some(HubChatsError::NotFound)
    );
}
