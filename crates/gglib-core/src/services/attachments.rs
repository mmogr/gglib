//! Attachment service: the one way an image is taken in, and read back.
//!
//! Every surface that attaches an image (the daemon's upload route, the
//! proxy's, the CLI) hands the file's bytes to [`AttachmentService::ingest`],
//! so what counts as an image and how large one may be is decided once.

use std::sync::Arc;

use crate::domain::attachment::{AttachmentBlob, AttachmentId, AttachmentInfo, AttachmentUpload};
use crate::ports::attachment_store::{AttachmentError, AttachmentStore};
use crate::request_pipeline::{MAX_IMAGE_BYTES, estimate_image_tokens, image_mime, image_size};

/// Service for storing and reading the images messages carry.
pub struct AttachmentService {
    store: Arc<dyn AttachmentStore>,
}

impl AttachmentService {
    /// Create a new attachment service.
    pub fn new(store: Arc<dyn AttachmentStore>) -> Self {
        Self { store }
    }

    /// The store itself, for the completion adapter, which reads the bytes
    /// of the images a request names through the port.
    #[must_use]
    pub fn store(&self) -> Arc<dyn AttachmentStore> {
        Arc::clone(&self.store)
    }

    /// Store `bytes` as an image, and answer what was stored and what it
    /// costs to send.
    ///
    /// The type is read from the first bytes, never from a name or a header
    /// the client sent, and the size from the image's own header. The bytes
    /// are stored exactly as they came, so the id is the hash of what the
    /// client holds; the same bytes sent again are the same one image and
    /// the same answer.
    ///
    /// # Errors
    ///
    /// [`AttachmentError::TooLarge`] over [`MAX_IMAGE_BYTES`];
    /// [`AttachmentError::Unsupported`] for anything but a PNG or a JPEG, and
    /// for one whose size cannot be read. Nothing is stored in either case.
    pub async fn ingest(&self, bytes: &[u8]) -> Result<AttachmentUpload, AttachmentError> {
        if bytes.len() > MAX_IMAGE_BYTES {
            return Err(AttachmentError::TooLarge);
        }
        let mime = image_mime(bytes).ok_or(AttachmentError::Unsupported)?;
        let (width, height) = image_size(bytes).ok_or(AttachmentError::Unsupported)?;
        let info = AttachmentInfo {
            id: AttachmentId::of(bytes),
            mime: mime.to_owned(),
            width,
            height,
        };
        self.store.put(&info, bytes).await?;
        Ok(AttachmentUpload {
            image_tokens: estimate_image_tokens(width, height),
            info,
        })
    }

    /// What is known of the image `id` without its bytes.
    ///
    /// # Errors
    ///
    /// [`AttachmentError::NotFound`] when no image has that id.
    pub async fn info(&self, id: &AttachmentId) -> Result<AttachmentInfo, AttachmentError> {
        self.store
            .info(id)
            .await?
            .ok_or_else(|| AttachmentError::NotFound(id.clone()))
    }

    /// The bytes of the image `id`, as they were sent.
    ///
    /// # Errors
    ///
    /// [`AttachmentError::NotFound`] when no image has that id.
    pub async fn blob(&self, id: &AttachmentId) -> Result<AttachmentBlob, AttachmentError> {
        self.store
            .blob(id)
            .await?
            .ok_or_else(|| AttachmentError::NotFound(id.clone()))
    }
}

#[cfg(test)]
#[path = "attachments_tests.rs"]
mod attachments_tests;
