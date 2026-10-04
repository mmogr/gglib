//! The images a request names, read from the store and written as data
//! URLs: the one place an image's id becomes its bytes.
//!
//! Every gglib surface names an image by its id
//! ([`AgentMessage::User`]'s `images`), so a chat's history is as long as
//! its text. A model reads `image_url` parts, so just before a request is
//! sent each id is read from the [`AttachmentStore`] and written as
//! `data:<mime>;base64,<bytes>`. A request for a paired machine's model is
//! resolved here too: its proxy is sent the parts, as any `OpenAI` client
//! sends them.
//!
//! Nothing here logs an image, and no error here carries one: only ids.

use std::collections::HashMap;
use std::sync::Arc;

use gglib_core::domain::AttachmentId;
use gglib_core::domain::agent::AgentMessage;
use gglib_core::ports::{AttachmentError, AttachmentStore};
use gglib_core::request_pipeline::MAX_REQUEST_IMAGE_BYTES;
use serde_json::{Value, json};

/// The data URL of each image one request names, by its id.
#[derive(Default)]
pub(super) struct ImageUrls(HashMap<AttachmentId, String>);

/// The ids a message carries; none for any but a user's.
fn images_of(message: &AgentMessage) -> &[AttachmentId] {
    match message {
        AgentMessage::User { images, .. } => images,
        _ => &[],
    }
}

/// Read every image `messages` name, history included.
///
/// An image named twice is read once and counted twice, as it is sent.
///
/// # Errors
///
/// [`AttachmentError::RequestTooLarge`] when the images are over
/// [`MAX_REQUEST_IMAGE_BYTES`] together, before the one that crosses the
/// limit is encoded; [`AttachmentError::NotFound`] for an id the store
/// lacks; [`AttachmentError::Storage`] when the store fails, or when the
/// messages name an image and no store was given.
pub(super) async fn resolve(
    store: Option<&Arc<dyn AttachmentStore>>,
    messages: &[AgentMessage],
) -> Result<ImageUrls, AttachmentError> {
    let mut urls = HashMap::new();
    let mut sizes: HashMap<&AttachmentId, usize> = HashMap::new();
    let mut total = 0_usize;
    for id in messages.iter().flat_map(images_of) {
        let size = if let Some(size) = sizes.get(id) {
            *size
        } else {
            let store = store.ok_or_else(|| {
                AttachmentError::Storage("no attachment store was given".to_owned())
            })?;
            let blob = store
                .blob(id)
                .await?
                .ok_or_else(|| AttachmentError::NotFound(id.clone()))?;
            if total + blob.data.len() <= MAX_REQUEST_IMAGE_BYTES {
                urls.insert(id.clone(), blob.data_url());
            }
            sizes.insert(id, blob.data.len());
            blob.data.len()
        };
        total += size;
        if total > MAX_REQUEST_IMAGE_BYTES {
            return Err(AttachmentError::RequestTooLarge);
        }
    }
    Ok(ImageUrls(urls))
}

impl ImageUrls {
    /// A user message's `content` on the wire.
    ///
    /// With no image it is the bare text, as it was before a message could
    /// carry one. With images it is an array: the text part first, left out
    /// when the text is empty, then one `image_url` part an image, in order.
    /// An id [`resolve`] did not read has no part.
    pub(super) fn user_content(&self, text: &str, images: &[AttachmentId]) -> Value {
        if images.is_empty() {
            return Value::String(text.to_owned());
        }
        let text = (!text.is_empty()).then(|| json!({ "type": "text", "text": text }));
        let images = images
            .iter()
            .filter_map(|id| self.0.get(id))
            .map(|url| json!({ "type": "image_url", "image_url": { "url": url } }));
        Value::Array(text.into_iter().chain(images).collect())
    }
}

#[cfg(test)]
#[path = "images_tests.rs"]
mod images_tests;
