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
//! One image no message names is written here too: the most recent picture
//! the drawing tool made, for a model that reads images, on the message
//! [`shown_picture`] says. It is read after the images the messages name
//! and never ends a request: a picture that is not stored, cannot be read,
//! or would take the request over the cap is left out, with its line.
//!
//! Nothing here logs an image, and no error here carries one: only ids.

use std::collections::HashMap;
use std::sync::Arc;

use gglib_core::domain::AttachmentId;
use gglib_core::domain::agent::AgentMessage;
use gglib_core::ports::{AttachmentError, AttachmentStore};
use gglib_core::request_pipeline::{DRAWN_PICTURE_LINE, MAX_REQUEST_IMAGE_BYTES, shown_picture};
use serde_json::{Value, json};

/// The data URL of each image one request names, by its id, and the drawn
/// picture the request shows its model.
#[derive(Default)]
pub(super) struct ImageUrls {
    named: HashMap<AttachmentId, String>,
    /// The index of the user message that carries the drawn picture, and
    /// the picture's data URL.
    shown: Option<(usize, String)>,
}

/// The ids a message carries; none for any but a user's.
fn images_of(message: &AgentMessage) -> &[AttachmentId] {
    match message {
        AgentMessage::User { images, .. } => images,
        _ => &[],
    }
}

/// Read every image `messages` name, history included, and then, for a
/// model that reads images (`image_input`), the drawn picture they show it.
///
/// An image named twice is read once and counted twice, as it is sent. The
/// drawn picture is counted with them, and is the one image left out when
/// the request would be over the cap with it: nobody attached it, so it
/// refuses nothing.
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
    image_input: bool,
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
    let shown = match (shown_picture(messages, image_input), store) {
        (Some(shown), Some(store)) => drawn(store.as_ref(), shown.image, total)
            .await
            .map(|url| (shown.at, url)),
        _ => None,
    };
    Ok(ImageUrls { named: urls, shown })
}

/// The drawn picture `id` as a data URL, when it can be read and fits under
/// [`MAX_REQUEST_IMAGE_BYTES`] beside the `named` bytes the request already
/// carries; `None` otherwise, whatever the reason.
async fn drawn(store: &dyn AttachmentStore, id: &AttachmentId, named: usize) -> Option<String> {
    let blob = store.blob(id).await.ok().flatten()?;
    (named + blob.data.len() <= MAX_REQUEST_IMAGE_BYTES).then(|| blob.data_url())
}

impl ImageUrls {
    /// The `content` on the wire of the user message at `index` of the
    /// request's messages.
    ///
    /// With no image it is the bare text, as it was before a message could
    /// carry one. With images it is an array: the text part first, left out
    /// when the text is empty, then one `image_url` part an image, in order.
    /// An id [`resolve`] did not read has no part.
    ///
    /// The message that carries the drawn picture is an array that starts
    /// with two more parts, [`DRAWN_PICTURE_LINE`] and the picture, before
    /// the person's own text and images.
    pub(super) fn user_content(&self, index: usize, text: &str, images: &[AttachmentId]) -> Value {
        let shown = self.shown.as_ref().filter(|(at, _)| *at == index);
        if images.is_empty() && shown.is_none() {
            return Value::String(text.to_owned());
        }
        let image = |url: &String| json!({ "type": "image_url", "image_url": { "url": url } });
        let shown = shown.into_iter().flat_map(|(_, url)| {
            [
                json!({ "type": "text", "text": DRAWN_PICTURE_LINE }),
                image(url),
            ]
        });
        let text = (!text.is_empty()).then(|| json!({ "type": "text", "text": text }));
        let images = images.iter().filter_map(|id| self.named.get(id)).map(image);
        Value::Array(shown.chain(text).chain(images).collect())
    }
}

#[cfg(test)]
#[path = "drawn_picture_tests.rs"]
mod drawn_picture_tests;
#[cfg(test)]
#[path = "far_images_tests.rs"]
mod far_images_tests;
#[cfg(test)]
#[path = "images_tests.rs"]
mod images_tests;
