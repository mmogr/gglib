//! What a user message's images become on the wire, and what stops a
//! request before it is sent.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use gglib_core::domain::agent::AgentMessage;
use gglib_core::domain::{AttachmentBlob, AttachmentId, AttachmentInfo};
use gglib_core::ports::{AttachmentError, AttachmentStore, LlmCompletionPort as _};
use gglib_core::request_pipeline::{MAX_IMAGE_BYTES, MAX_REQUEST_IMAGE_BYTES};
use serde_json::{Value, json};

use super::super::LlmCompletionAdapter;
use super::super::body::build_chat_body;
use super::resolve;

/// A store of what a test put in it.
#[derive(Default)]
struct Kept(HashMap<AttachmentId, AttachmentBlob>);

impl Kept {
    /// Keep `data` as `mime`, and answer its id.
    fn keep(&mut self, mime: &str, data: Vec<u8>) -> AttachmentId {
        let id = AttachmentId::of(&data);
        let mime = mime.to_owned();
        self.0.insert(id.clone(), AttachmentBlob { mime, data });
        id
    }

    fn store(self) -> Arc<dyn AttachmentStore> {
        Arc::new(self)
    }
}

#[async_trait]
impl AttachmentStore for Kept {
    async fn put(&self, _info: &AttachmentInfo, _bytes: &[u8]) -> Result<(), AttachmentError> {
        unreachable!("the adapter only reads")
    }

    async fn info(&self, _id: &AttachmentId) -> Result<Option<AttachmentInfo>, AttachmentError> {
        unreachable!("the adapter reads bytes, not facts")
    }

    async fn size(&self, _id: &AttachmentId) -> Result<Option<usize>, AttachmentError> {
        unreachable!("the adapter reads bytes, not sizes")
    }

    async fn blob(&self, id: &AttachmentId) -> Result<Option<AttachmentBlob>, AttachmentError> {
        Ok(self.0.get(id).cloned())
    }
}

fn user(content: &str, images: &[&AttachmentId]) -> AgentMessage {
    AgentMessage::User {
        content: content.to_owned(),
        images: images.iter().map(|id| (*id).clone()).collect(),
    }
}

/// The `messages` of the body the adapter builds for `messages`.
async fn wire(store: &Arc<dyn AttachmentStore>, messages: &[AgentMessage]) -> Value {
    let images = resolve(Some(store), messages).await.unwrap();
    build_chat_body("m", messages, &[], None, &images)["messages"].take()
}

/// A message with no image is on the wire what it was before a message
/// could carry one: `content` is the bare string, with or without a store.
#[tokio::test]
async fn a_message_with_no_image_is_its_bare_text() {
    let messages = [user("hello", &[])];
    let want = json!([{ "role": "user", "content": "hello" }]);

    assert_eq!(wire(&Kept::default().store(), &messages).await, want);
    let images = resolve(None, &messages).await.unwrap();
    let body = build_chat_body("m", &messages, &[], None, &images);
    assert_eq!(body["messages"], want);
    assert_eq!(
        body,
        json!({
            "model": "m",
            "messages": [{ "role": "user", "content": "hello" }],
            "stream": true,
            "return_progress": true,
            "stream_options": { "include_usage": true },
        })
    );
}

/// With images the content is parts: the text first, then each image in the
/// message's order as a base64 data URL of its stored type and bytes.
#[tokio::test]
async fn text_and_images_are_the_text_part_then_each_image_in_order() {
    let mut kept = Kept::default();
    let png = kept.keep("image/png", b"first image".to_vec());
    let jpeg = kept.keep("image/jpeg", vec![0xFF, 0xD8, 0xFF, 0x00]);
    let store = kept.store();

    let sent = wire(&store, &[user("what is this?", &[&jpeg, &png])]).await;

    assert_eq!(
        sent,
        json!([{
            "role": "user",
            "content": [
                { "type": "text", "text": "what is this?" },
                { "type": "image_url", "image_url": { "url": "data:image/jpeg;base64,/9j/AA==" } },
                { "type": "image_url", "image_url": {
                    "url": "data:image/png;base64,Zmlyc3QgaW1hZ2U="
                } },
            ],
        }])
    );
}

/// A message that is its image alone has no text part: an empty one is
/// never sent.
#[tokio::test]
async fn an_image_alone_has_no_text_part() {
    let mut kept = Kept::default();
    let png = kept.keep("image/png", b"first image".to_vec());

    let sent = wire(&kept.store(), &[user("", &[&png])]).await;

    assert_eq!(
        sent[0]["content"],
        json!([{ "type": "image_url", "image_url": {
            "url": "data:image/png;base64,Zmlyc3QgaW1hZ2U="
        } }])
    );
}

/// An image in the history is sent again with its turn, and only user
/// messages carry any.
#[tokio::test]
async fn an_image_in_the_history_is_resolved_with_its_turn() {
    let mut kept = Kept::default();
    let png = kept.keep("image/png", b"first image".to_vec());
    let messages = [
        user("look", &[&png]),
        AgentMessage::Tool {
            tool_call_id: "c1".to_owned(),
            content: "ok".to_owned(),
        },
        user("and now?", &[]),
    ];

    let sent = wire(&kept.store(), &messages).await;

    assert_eq!(sent[0]["content"][1]["type"], "image_url");
    assert_eq!(sent[1]["content"], "ok");
    assert_eq!(sent[2]["content"], "and now?");
}

/// A loopback port nothing answers on, to see whether anything dialled it.
struct Upstream(tokio::net::TcpListener);

impl Upstream {
    async fn bind() -> Self {
        Self(tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap())
    }

    /// An adapter pointed here, reading images from `store`.
    fn adapter(&self, store: Option<Arc<dyn AttachmentStore>>) -> LlmCompletionAdapter {
        let base = format!("http://{}", self.0.local_addr().unwrap());
        LlmCompletionAdapter::new(base, None).with_attachments(store)
    }

    /// Whether a connection arrived. A request that was sent connects first.
    async fn was_dialled(&self) -> bool {
        let wait = std::time::Duration::from_millis(200);
        tokio::time::timeout(wait, self.0.accept()).await.is_ok()
    }
}

/// The refusal `chat_stream` ends with, as the port's caller holds it.
///
/// Bounded: a request that was sent instead waits on a port that never
/// answers.
async fn refused(adapter: &LlmCompletionAdapter, messages: &[AgentMessage]) -> anyhow::Error {
    let wait = std::time::Duration::from_secs(5);
    tokio::time::timeout(wait, adapter.chat_stream(messages, &[]))
        .await
        .expect("the request was sent, not refused")
        .err()
        .expect("the request is refused")
}

/// Images over the request's limit together are the named refusal, and the
/// server is sent nothing. At the limit exactly, the request goes.
#[tokio::test]
async fn images_over_the_request_limit_are_refused_before_anything_is_sent() {
    let mut kept = Kept::default();
    let full = kept.keep("image/png", vec![1; MAX_IMAGE_BYTES]);
    let also_full = kept.keep("image/png", vec![2; MAX_IMAGE_BYTES]);
    let one_byte = kept.keep("image/png", vec![3]);
    let store = kept.store();
    assert_eq!(MAX_REQUEST_IMAGE_BYTES, 2 * MAX_IMAGE_BYTES);

    let upstream = Upstream::bind().await;
    let adapter = upstream.adapter(Some(Arc::clone(&store)));
    let over = [user("one", &[&full]), user("two", &[&also_full, &one_byte])];

    let error = refused(&adapter, &over).await;

    assert!(matches!(
        error.downcast_ref::<AttachmentError>(),
        Some(AttachmentError::RequestTooLarge)
    ));
    assert!(!upstream.was_dialled().await);
    let shown = format!("{error:#} {error:?}");
    assert!(shown.contains("16 MiB"), "{shown}");
    assert!(
        shown.len() < 1000,
        "an error carries no image: {}",
        shown.len()
    );
    assert!(!shown.contains("AQEBAQEB"), "nor any of one's base64");

    let at = [user("one", &[&full]), user("two", &[&also_full])];
    assert!(resolve(Some(&store), &at).await.is_ok());
}

/// An image named in two messages is sent twice, so it is counted twice.
#[tokio::test]
async fn an_image_named_twice_counts_twice() {
    let mut kept = Kept::default();
    let full = kept.keep("image/png", vec![1; MAX_IMAGE_BYTES]);
    let store = kept.store();
    let twice = [user("one", &[&full]), user("again", &[&full])];
    let thrice = [user("one", &[&full, &full]), user("again", &[&full])];

    assert!(resolve(Some(&store), &twice).await.is_ok());
    assert!(matches!(
        resolve(Some(&store), &thrice).await,
        Err(AttachmentError::RequestTooLarge)
    ));
}

/// An id the store lacks is refused by that id, and nothing is sent.
#[tokio::test]
async fn an_id_the_store_lacks_is_refused_before_anything_is_sent() {
    let never_stored = AttachmentId::of(b"never stored");
    let upstream = Upstream::bind().await;
    let adapter = upstream.adapter(Some(Kept::default().store()));

    let error = refused(&adapter, &[user("look", &[&never_stored])]).await;

    assert!(matches!(
        error.downcast_ref::<AttachmentError>(),
        Some(AttachmentError::NotFound(id)) if *id == never_stored
    ));
    assert!(!upstream.was_dialled().await);
}

/// A message with no image does reach the port: the three refusals above
/// are told apart from a port nothing would dial.
#[tokio::test]
async fn a_request_with_no_refusal_is_sent() {
    let upstream = Upstream::bind().await;
    let adapter = upstream
        .adapter(None)
        .with_retry_policy(gglib_core::retry::RetryPolicy {
            max_attempts: 1,
            ..gglib_core::retry::RetryPolicy::default()
        });
    let messages = [user("hello", &[])];

    let (dialled, ()) = tokio::join!(upstream.was_dialled(), async {
        let send = adapter.chat_stream(&messages, &[]);
        let _ = tokio::time::timeout(std::time::Duration::from_millis(300), send).await;
    });

    assert!(dialled);
}

/// An adapter given no store sends text as ever, and ends a request with an
/// image rather than send it without one.
#[tokio::test]
async fn an_adapter_with_no_store_sends_no_message_that_names_an_image() {
    let id = AttachmentId::of(b"an image");
    let upstream = Upstream::bind().await;
    let adapter = upstream.adapter(None);

    let error = refused(&adapter, &[user("look", &[&id])]).await;

    assert!(matches!(
        error.downcast_ref::<AttachmentError>(),
        Some(AttachmentError::Storage(_))
    ));
    assert!(!upstream.was_dialled().await);
}
