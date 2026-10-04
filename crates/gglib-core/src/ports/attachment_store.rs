//! Attachment store port definition.
//!
//! Where an image a user message carries is kept: its bytes once, under the
//! hash of them. A message is linked to the images it carries by the chat
//! history repository, in the transaction that saves the message.

use async_trait::async_trait;
use thiserror::Error;

use crate::domain::attachment::{AttachmentBlob, AttachmentId, AttachmentInfo};
use crate::request_pipeline::{MAX_IMAGE_BYTES, MAX_REQUEST_IMAGE_BYTES};

/// Why an image was not stored, found or sent. Each refusal has a
/// [`code`](Self::code) a client matches on.
#[derive(Debug, Error)]
pub enum AttachmentError {
    /// The image is over [`MAX_IMAGE_BYTES`].
    #[error("The image is larger than {} MiB, the most one image may be.", MAX_IMAGE_BYTES >> 20)]
    TooLarge,

    /// The bytes are not a PNG or a JPEG whose size can be read.
    #[error("The file is not an image that can be attached: only PNG and JPEG are read.")]
    Unsupported,

    /// No stored image has this id.
    #[error("No stored image has the id {0}. Attach the image again.")]
    NotFound(AttachmentId),

    /// The images of one request are over [`MAX_REQUEST_IMAGE_BYTES`]
    /// together.
    #[error(
        "The images in this chat are more than {} MiB together, the most one request may carry. \
         Start a new chat, or send fewer or smaller images.",
        MAX_REQUEST_IMAGE_BYTES >> 20
    )]
    RequestTooLarge,

    /// The store failed.
    #[error("Attachment storage error: {0}")]
    Storage(String),
}

impl AttachmentError {
    /// The error code a client matches on; `None` for a failure of the store
    /// itself, which is no refusal of the request.
    #[must_use]
    pub const fn code(&self) -> Option<&'static str> {
        match self {
            Self::TooLarge => Some("image_too_large"),
            Self::Unsupported => Some("unsupported_image"),
            Self::NotFound(_) => Some("attachment_not_found"),
            Self::RequestTooLarge => Some("request_images_too_large"),
            Self::Storage(_) => None,
        }
    }

    /// The HTTP status it is answered with. An id the store lacks is 400 and
    /// not 404: on the routes that take a turn, a paired device reads a 404
    /// as a hub that has no such route.
    #[must_use]
    pub const fn http_status(&self) -> u16 {
        match self {
            Self::TooLarge => 413,
            Self::Unsupported | Self::NotFound(_) | Self::RequestTooLarge => 400,
            Self::Storage(_) => 500,
        }
    }

    /// The HTTP status it is answered with on a route that reads an image
    /// by its id: there an id the store lacks is the 404 of any path that
    /// names nothing.
    #[must_use]
    pub const fn fetch_status(&self) -> u16 {
        match self {
            Self::NotFound(_) => 404,
            _ => self.http_status(),
        }
    }
}

/// Port for keeping images by the hash of their bytes.
#[async_trait]
pub trait AttachmentStore: Send + Sync {
    /// Keep `bytes` under `info.id`. An id already kept is left as it is:
    /// the id is the hash of the bytes, so they are the same bytes.
    async fn put(&self, info: &AttachmentInfo, bytes: &[u8]) -> Result<(), AttachmentError>;

    /// What is known of the image `id` without its bytes, or `None` when no
    /// image has that id.
    async fn info(&self, id: &AttachmentId) -> Result<Option<AttachmentInfo>, AttachmentError>;

    /// The bytes of the image `id`, or `None` when no image has that id.
    async fn blob(&self, id: &AttachmentId) -> Result<Option<AttachmentBlob>, AttachmentError>;
}
