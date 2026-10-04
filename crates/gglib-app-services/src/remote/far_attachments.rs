//! The far machine's stored images: one sent to it, one read back.
//!
//! Both go through the streaming client. An image is megabytes, and a
//! tunnel that is relayed carries it slowly: the bounded client's limit on a
//! whole exchange would cut a healthy upload off, where the streaming
//! client's limit is on silence alone. Nothing here logs or keeps a byte of
//! an image.

use gglib_core::contracts::http::attachments::PROXY_ATTACHMENTS_PATH;
use gglib_core::domain::AttachmentId;

use super::FarProxy;
use crate::error::GuiError;

impl FarProxy {
    /// `POST /v1/attachments` with `bytes` as the body: an image stored
    /// there, for a turn on one of its chats to name. `content_type` is the
    /// caller's and is passed on as it came; the far machine reads the type
    /// from the bytes.
    ///
    /// # Errors
    ///
    /// `Unavailable` when the request did not get through.
    pub async fn upload_attachment(
        &self,
        bytes: impl Into<reqwest::Body>,
        content_type: Option<&str>,
    ) -> Result<reqwest::Response, GuiError> {
        let mut request = self
            .streaming
            .post(self.url(PROXY_ATTACHMENTS_PATH))
            .body(bytes);
        if let Some(content_type) = content_type {
            request = request.header(reqwest::header::CONTENT_TYPE, content_type);
        }
        self.send(request).await
    }

    /// `GET /v1/attachments/{id}`: the image's bytes, as they were sent.
    ///
    /// # Errors
    ///
    /// `Unavailable` when the request did not get through.
    pub async fn fetch_attachment(&self, id: &AttachmentId) -> Result<reqwest::Response, GuiError> {
        let path = format!("{PROXY_ATTACHMENTS_PATH}/{id}");
        self.send(self.streaming.get(self.url(&path))).await
    }
}

#[cfg(test)]
#[path = "far_attachments_tests.rs"]
mod far_attachments_tests;
