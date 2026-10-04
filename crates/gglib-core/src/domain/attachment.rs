//! An image a user message carries, named by the hash of its bytes.
//!
//! gglib's own surfaces send an image once and name it afterwards: a
//! message holds [`AttachmentId`]s, never bytes, so a chat's history is as
//! long as its text however many screenshots it has. What a client is told
//! about a stored image is [`AttachmentInfo`]; the bytes are read by id.

use std::fmt;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// The length of an [`AttachmentId`]: a SHA-256 digest in hex.
const ID_LEN: usize = 64;

/// The id of a stored image: the SHA-256 of its bytes, in lowercase hex.
///
/// The bytes are stored as they were sent, so a client can compute the id of
/// an image it holds, and the same image sent twice is one id.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct AttachmentId(String);

/// A string that is not an [`AttachmentId`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("an attachment id is 64 lowercase hex characters")]
pub struct InvalidAttachmentId;

impl AttachmentId {
    /// The id of `bytes`.
    #[must_use]
    pub fn of(bytes: &[u8]) -> Self {
        Self(format!("{:x}", Sha256::digest(bytes)))
    }

    /// `text` as an id.
    ///
    /// # Errors
    ///
    /// [`InvalidAttachmentId`] unless `text` is 64 characters, each a digit
    /// or one of `a` to `f`.
    pub fn parse(text: &str) -> Result<Self, InvalidAttachmentId> {
        Self::try_from(text.to_owned())
    }

    /// The id as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for AttachmentId {
    type Error = InvalidAttachmentId;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        let hex = |byte: &u8| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte);
        if text.len() == ID_LEN && text.as_bytes().iter().all(hex) {
            Ok(Self(text))
        } else {
            Err(InvalidAttachmentId)
        }
    }
}

impl From<AttachmentId> for String {
    fn from(id: AttachmentId) -> Self {
        id.0
    }
}

impl fmt::Display for AttachmentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// What is known of a stored image without its bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct AttachmentInfo {
    /// The image's id.
    #[cfg_attr(feature = "ts-bindings", ts(type = "string"))]
    pub id: AttachmentId,
    /// `image/png` or `image/jpeg`, as its first bytes say.
    pub mime: String,
    /// Its width in pixels.
    pub width: u32,
    /// Its height in pixels.
    pub height: u32,
}

/// The answer to an upload: the stored image, and what it costs to send.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct AttachmentUpload {
    /// The stored image.
    #[serde(flatten)]
    pub info: AttachmentInfo,
    /// The prompt tokens it is estimated to take
    /// ([`crate::request_pipeline::estimate_image_tokens`]). Worked out from
    /// its size at each upload, and never stored.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub image_tokens: usize,
}

/// A stored image's bytes, as they were sent, and the type they are served
/// with.
#[derive(Clone, PartialEq, Eq)]
pub struct AttachmentBlob {
    /// `image/png` or `image/jpeg`.
    pub mime: String,
    /// The image.
    pub data: Vec<u8>,
}

impl AttachmentBlob {
    /// The image as an `OpenAI` `image_url`: `data:<mime>;base64,<bytes>`.
    /// What the completion adapter sends a model in place of the id.
    #[must_use]
    pub fn data_url(&self) -> String {
        use base64::Engine as _;
        let payload = base64::engine::general_purpose::STANDARD.encode(&self.data);
        format!("data:{};base64,{payload}", self.mime)
    }
}

/// The length stands in for the bytes: an image is never written to a log.
impl fmt::Debug for AttachmentBlob {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AttachmentBlob")
            .field("mime", &self.mime)
            .field("bytes", &self.data.len())
            .finish()
    }
}

#[cfg(test)]
#[path = "attachment_tests.rs"]
mod attachment_tests;
