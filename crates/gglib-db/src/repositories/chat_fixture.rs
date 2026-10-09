//! Chats for the branch tests: a database, its repository, the service
//! over it, and the chat they start from.

use std::sync::Arc;

use sqlx::SqlitePool;

use gglib_core::domain::attachment::{AttachmentId, AttachmentInfo};
use gglib_core::domain::chat::{ConversationSettings, MessageRole, NewConversation, NewMessage};
use gglib_core::ports::attachment_store::AttachmentStore;
use gglib_core::ports::chat_history::ChatHistoryRepository;
use gglib_core::services::ChatHistoryService;

use super::{SqliteAttachmentStore, SqliteChatHistoryRepository};
use crate::setup::setup_test_database;

use MessageRole::{Assistant, User};

pub(super) struct Chats {
    pub(super) repo: Arc<SqliteChatHistoryRepository>,
    store: SqliteAttachmentStore,
    pool: SqlitePool,
}

pub(super) async fn chats() -> Chats {
    let pool = setup_test_database().await.expect("setup_test_database");
    Chats {
        repo: Arc::new(SqliteChatHistoryRepository::new(pool.clone())),
        store: SqliteAttachmentStore::new(pool.clone()),
        pool,
    }
}

impl Chats {
    pub(super) fn service(&self) -> ChatHistoryService {
        ChatHistoryService::new(Arc::clone(&self.repo) as Arc<dyn ChatHistoryRepository>)
    }

    /// A chat titled `title` whose messages are `rows`, in order; its id and
    /// theirs.
    pub(super) async fn chat(&self, title: &str, rows: &[(MessageRole, &str)]) -> (i64, Vec<i64>) {
        let settings = ConversationSettings {
            temperature: Some(0.2),
            ..ConversationSettings::default()
        };
        let id = self
            .repo
            .create_conversation(NewConversation {
                title: title.to_owned(),
                system_prompt: Some("Be brief.".to_owned()),
                settings: Some(settings),
                ..NewConversation::default()
            })
            .await
            .unwrap();
        let mut ids = Vec::new();
        for (role, content) in rows {
            ids.push(
                self.repo
                    .save_message(message(id, *role, content))
                    .await
                    .unwrap(),
            );
        }
        (id, ids)
    }

    pub(super) async fn image(&self, name: &str) -> AttachmentId {
        let info = AttachmentInfo {
            id: AttachmentId::of(name.as_bytes()),
            mime: "image/png".to_owned(),
            width: 10,
            height: 10,
        };
        self.store.put(&info, name.as_bytes()).await.unwrap();
        info.id
    }

    pub(super) async fn contents(&self, id: i64) -> Vec<String> {
        let rows = self.repo.get_messages(id).await.unwrap();
        rows.into_iter().map(|row| row.content).collect()
    }

    pub(super) async fn count(&self, table: &str) -> i64 {
        sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }
}

pub(super) fn message(conversation_id: i64, role: MessageRole, content: &str) -> NewMessage {
    NewMessage {
        conversation_id,
        role,
        content: content.to_owned(),
        metadata: None,
        images: Vec::new(),
    }
}

pub(super) const KYOTO: [(MessageRole, &str); 4] = [
    (User, "Plan a trip to Kyoto"),
    (Assistant, "Day 1: temples"),
    (User, "Make it cheaper"),
    (Assistant, "Hostels and buses."),
];
