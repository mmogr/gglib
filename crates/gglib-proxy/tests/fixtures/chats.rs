//! A stand-in for the hub's chats, for the proxy's `/v1/chats` tests.
//!
//! Counts every call, so a test can say whether a request reached the chats
//! at all, and answers from a fixed pair of chats. Its images go through
//! the real ingest, into a map.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use gglib_core::domain::chat::{Conversation, Message, MessageRole};
use gglib_core::domain::hub_chats::{HubChat, HubChatList, HubChatOpen};
use gglib_core::domain::{AttachmentBlob, AttachmentId, AttachmentInfo, AttachmentUpload};
use gglib_core::ports::{AttachmentError, AttachmentStore, HubChatsError, HubChatsPort};
use gglib_core::services::AttachmentService;
use gglib_core::{CorsConfig, DevicePorts, ProxyAccessConfig};

/// The one chat that opens.
pub(crate) const OPEN_ID: i64 = 7;

/// The stub.
#[derive(Debug, Default)]
pub(crate) struct FakeChats {
    pub(crate) calls: AtomicUsize,
    images: Arc<Images>,
}

/// The images the stub holds, by id.
#[derive(Default)]
struct Images(Mutex<HashMap<AttachmentId, (AttachmentInfo, Vec<u8>)>>);

impl std::fmt::Debug for Images {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Images").finish_non_exhaustive()
    }
}

#[async_trait]
impl AttachmentStore for Images {
    async fn put(&self, info: &AttachmentInfo, bytes: &[u8]) -> Result<(), AttachmentError> {
        let kept = (info.clone(), bytes.to_vec());
        self.0
            .lock()
            .unwrap()
            .entry(info.id.clone())
            .or_insert(kept);
        Ok(())
    }

    async fn info(&self, id: &AttachmentId) -> Result<Option<AttachmentInfo>, AttachmentError> {
        Ok(self.0.lock().unwrap().get(id).map(|held| held.0.clone()))
    }

    async fn size(&self, id: &AttachmentId) -> Result<Option<usize>, AttachmentError> {
        Ok(self.0.lock().unwrap().get(id).map(|held| held.1.len()))
    }

    async fn blob(&self, id: &AttachmentId) -> Result<Option<AttachmentBlob>, AttachmentError> {
        let held = self.0.lock().unwrap().get(id).cloned();
        Ok(held.map(|(info, data)| AttachmentBlob {
            mime: info.mime,
            data,
        }))
    }

    async fn ids_starting_with(&self, prefix: &str) -> Result<Vec<AttachmentId>, AttachmentError> {
        let held = self.0.lock().unwrap();
        Ok(gglib_core::ports::attachment_store::ids_starting_with(
            held.keys(),
            prefix,
        ))
    }
}

/// A PNG's signature and `IHDR` for `width` by `height`, then `padding`
/// zero bytes: a file the ingest reads the size of.
pub(crate) fn png(width: u32, height: u32, padding: usize) -> Vec<u8> {
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    bytes.extend([0, 0, 0, 13]);
    bytes.extend(b"IHDR");
    bytes.extend(width.to_be_bytes());
    bytes.extend(height.to_be_bytes());
    bytes.extend([8, 6, 0, 0, 0, 0, 0, 0, 0]);
    bytes.resize(bytes.len() + padding, 0);
    bytes
}

impl FakeChats {
    pub(crate) fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    /// How many images the stub holds.
    pub(crate) fn images(&self) -> usize {
        self.images.0.lock().unwrap().len()
    }

    fn attachments(&self) -> AttachmentService {
        AttachmentService::new(Arc::clone(&self.images) as Arc<dyn AttachmentStore>)
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
            branch_of: None,
            lineage_id: None,
        },
        messages: vec![Message {
            id: 1,
            conversation_id: OPEN_ID,
            origin_id: None,
            role: MessageRole::User,
            content: "why".to_owned(),
            created_at: "2026-09-30 09:12:00".to_owned(),
            metadata: None,
            images: vec![AttachmentInfo {
                id: AttachmentId::of(b"a screenshot"),
                mime: "image/png".to_owned(),
                width: 640,
                height: 480,
            }],
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

    async fn attach(&self, bytes: &[u8]) -> Result<AttachmentUpload, AttachmentError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.attachments().ingest(bytes).await
    }

    async fn attachment(&self, id: &AttachmentId) -> Result<AttachmentBlob, AttachmentError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.attachments().blob(id).await
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
    .with_devices(DevicePorts {
        chats: chats.map(|c| c as Arc<dyn HubChatsPort>),
        ..DevicePorts::default()
    });
    let (base, _, cancel) = super::access::spawn_proxy(access).await;
    (base, cancel)
}
