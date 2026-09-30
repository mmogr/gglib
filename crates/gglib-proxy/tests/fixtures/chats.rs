//! A stand-in for the hub's chats, for the proxy's `/v1/chats` tests.
//!
//! Counts every call, so a test can say whether a request reached the chats
//! at all, and answers from a fixed pair of chats.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use gglib_core::domain::chat::{Conversation, Message, MessageRole};
use gglib_core::domain::hub_chats::{HubChat, HubChatList, HubChatOpen};
use gglib_core::ports::{HubChatsError, HubChatsPort};
use gglib_core::{CorsConfig, ProxyAccessConfig};

/// The one chat that opens.
pub(crate) const OPEN_ID: i64 = 7;

/// The stub.
#[derive(Debug, Default)]
pub(crate) struct FakeChats {
    pub(crate) calls: AtomicUsize,
}

impl FakeChats {
    pub(crate) fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

/// What every listing answers with.
pub(crate) fn listed() -> HubChatList {
    HubChatList {
        chats: vec![
            HubChat {
                id: OPEN_ID,
                title: "Why the build broke".to_owned(),
                model_id: Some(3),
                model: Some("qwen3-8b".to_owned()),
                updated_at: "2026-09-30 09:13:00".to_owned(),
                live_run: Some("chat-1".to_owned()),
            },
            HubChat {
                id: 2,
                title: "Older".to_owned(),
                model_id: None,
                model: None,
                updated_at: "2026-09-29 18:00:00".to_owned(),
                live_run: None,
            },
        ],
    }
}

/// What opening [`OPEN_ID`] answers with.
pub(crate) fn opened() -> HubChatOpen {
    HubChatOpen {
        conversation: Conversation {
            id: OPEN_ID,
            title: "Why the build broke".to_owned(),
            model_id: Some(3),
            system_prompt: None,
            settings: None,
            created_at: "2026-09-30 09:12:00".to_owned(),
            updated_at: "2026-09-30 09:13:00".to_owned(),
        },
        messages: vec![Message {
            id: 1,
            conversation_id: OPEN_ID,
            role: MessageRole::User,
            content: "why".to_owned(),
            created_at: "2026-09-30 09:12:00".to_owned(),
            metadata: None,
        }],
    }
}

#[async_trait]
impl HubChatsPort for FakeChats {
    async fn list(&self) -> Result<HubChatList, HubChatsError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(listed())
    }

    async fn open(&self, id: i64) -> Result<HubChatOpen, HubChatsError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if id == OPEN_ID {
            Ok(opened())
        } else {
            Err(HubChatsError::NotFound)
        }
    }
}

/// The real proxy holding `chats`, demanding `key` when there is one.
pub(crate) async fn serve(
    key: Option<&str>,
    chats: Option<Arc<FakeChats>>,
) -> (String, tokio_util::sync::CancellationToken) {
    let access = ProxyAccessConfig::new(
        CorsConfig::LocalOnly,
        key.map(str::to_owned),
        "127.0.0.1",
        vec![],
    )
    .with_chats(chats.map(|c| c as Arc<dyn HubChatsPort>));
    let (base, _, cancel) = super::access::spawn_proxy(access).await;
    (base, cancel)
}
