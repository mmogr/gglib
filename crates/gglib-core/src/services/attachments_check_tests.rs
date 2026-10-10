//! The check of a request's images before its run is made: every id the
//! messages name, history included, is stored, and together they are no
//! more than the cap. Only sizes are read.

use std::collections::BTreeMap;
use std::sync::Mutex;

use async_trait::async_trait;

use super::*;
use crate::domain::agent::AssistantContent;

/// A store that holds only sizes, counts the times it is asked for one,
/// and is never asked for an image's bytes.
#[derive(Default)]
struct Sizes {
    kept: BTreeMap<AttachmentId, usize>,
    asked: Mutex<usize>,
}

impl Sizes {
    fn with(sizes: &[(&AttachmentId, usize)]) -> Arc<Self> {
        let kept = sizes.iter().map(|(id, size)| ((*id).clone(), *size));
        Arc::new(Self {
            kept: kept.collect(),
            asked: Mutex::new(0),
        })
    }
}

#[async_trait]
impl AttachmentStore for Sizes {
    async fn put(&self, _info: &AttachmentInfo, _bytes: &[u8]) -> Result<(), AttachmentError> {
        unreachable!("the check only reads")
    }

    async fn info(&self, _id: &AttachmentId) -> Result<Option<AttachmentInfo>, AttachmentError> {
        unreachable!("the check reads sizes, not facts")
    }

    async fn size(&self, id: &AttachmentId) -> Result<Option<usize>, AttachmentError> {
        *self.asked.lock().unwrap() += 1;
        Ok(self.kept.get(id).copied())
    }

    async fn blob(&self, _id: &AttachmentId) -> Result<Option<AttachmentBlob>, AttachmentError> {
        unreachable!("the check never reads an image's bytes")
    }

    async fn ids_starting_with(&self, prefix: &str) -> Result<Vec<AttachmentId>, AttachmentError> {
        Ok(crate::ports::attachment_store::ids_starting_with(
            self.kept.keys(),
            prefix,
        ))
    }
}

fn user(images: &[&AttachmentId]) -> AgentMessage {
    AgentMessage::User {
        content: "look".to_owned(),
        images: images.iter().map(|id| (*id).clone()).collect(),
    }
}

fn assistant() -> AgentMessage {
    AgentMessage::Assistant {
        content: AssistantContent {
            text: Some("seen".to_owned()),
            tool_calls: Vec::new(),
        },
    }
}

async fn check(store: &Arc<Sizes>, messages: &[AgentMessage]) -> Result<(), AttachmentError> {
    let store: Arc<dyn AttachmentStore> = store.clone();
    AttachmentService::new(store).check_request(messages).await
}

#[tokio::test]
async fn messages_with_no_image_ask_the_store_nothing() {
    let store = Sizes::with(&[]);
    let messages = [user(&[]), assistant(), user(&[])];

    check(&store, &messages).await.unwrap();

    assert_eq!(*store.asked.lock().unwrap(), 0);
}

/// An id an earlier message names, not only the last, is looked up: the
/// whole history is sent again.
#[tokio::test]
async fn an_id_the_history_names_that_is_not_stored_is_the_not_found_refusal() {
    let kept = AttachmentId::of(b"kept");
    let gone = AttachmentId::of(b"swept");
    let store = Sizes::with(&[(&kept, 10)]);
    let messages = [user(&[&gone]), assistant(), user(&[&kept])];

    let refusal = check(&store, &messages).await.unwrap_err();

    assert!(matches!(&refusal, AttachmentError::NotFound(id) if *id == gone));
    assert_eq!(refusal.code(), Some("attachment_not_found"));
}

/// The images of every message add up: the history's image and the new
/// one together are refused where either alone would pass.
#[tokio::test]
async fn the_historys_images_count_toward_the_total() {
    let old = AttachmentId::of(b"old");
    let new = AttachmentId::of(b"new");
    let store = Sizes::with(&[(&old, MAX_REQUEST_IMAGE_BYTES), (&new, 1)]);

    check(&store, &[user(&[&old])]).await.unwrap();
    check(&store, &[user(&[&new])]).await.unwrap();
    let refusal = check(&store, &[user(&[&old]), assistant(), user(&[&new])])
        .await
        .unwrap_err();

    assert!(matches!(refusal, AttachmentError::RequestTooLarge));
    assert_eq!(refusal.code(), Some("request_images_too_large"));
}

/// Images exactly at the cap together are sent; one byte more is refused.
#[tokio::test]
async fn images_at_the_cap_pass_and_one_byte_over_is_refused() {
    let half = AttachmentId::of(b"half");
    let other = AttachmentId::of(b"other half");
    let more = AttachmentId::of(b"other half and a byte");
    let store = Sizes::with(&[
        (&half, MAX_REQUEST_IMAGE_BYTES / 2),
        (&other, MAX_REQUEST_IMAGE_BYTES / 2),
        (&more, MAX_REQUEST_IMAGE_BYTES / 2 + 1),
    ]);

    check(&store, &[user(&[&half, &other])]).await.unwrap();
    let refusal = check(&store, &[user(&[&half, &more])]).await.unwrap_err();

    assert!(matches!(refusal, AttachmentError::RequestTooLarge));
}

/// An image named twice is sent twice, so it counts twice; its size is
/// asked for once.
#[tokio::test]
async fn an_image_named_twice_counts_twice_and_is_looked_up_once() {
    let image = AttachmentId::of(b"image");
    let store = Sizes::with(&[(&image, MAX_REQUEST_IMAGE_BYTES / 2 + 1)]);

    let refusal = check(&store, &[user(&[&image]), assistant(), user(&[&image])])
        .await
        .unwrap_err();

    assert!(matches!(refusal, AttachmentError::RequestTooLarge));
    assert_eq!(*store.asked.lock().unwrap(), 1);
}
