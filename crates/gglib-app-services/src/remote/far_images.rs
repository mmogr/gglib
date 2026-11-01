//! The far machine's drawing: a render, and whether it can draw.
//!
//! A render is read as a stream of events for as long as it takes, so it
//! goes through the streaming client, whose limit is on silence alone.

use gglib_core::contracts::http::images::ImageGenerationsRequest;

use super::FarProxy;
use crate::error::GuiError;

impl FarProxy {
    /// `POST /v1/images/generations` with `request`: a render that machine
    /// draws with its own image model, answered as soon as its head arrives.
    /// With `stream` set the body is the render's events, read as it goes.
    ///
    /// # Errors
    ///
    /// `Unavailable` when the request did not get through.
    pub async fn generate_images(
        &self,
        request: &ImageGenerationsRequest,
    ) -> Result<reqwest::Response, GuiError> {
        let mut post = self
            .streaming
            .post(self.url("/images/generations"))
            .json(request);
        if request.stream {
            post = post.header(reqwest::header::ACCEPT, "text/event-stream");
        }
        self.send(post).await
    }

    /// `GET /v1/images/drawing`: whether that machine can draw for a turn
    /// sent with Draw pressed, why not, and its image models; with
    /// `image_model`, one of its ids, as `?image_model=<id>`, so it judges
    /// that choice. An older hub answers 404.
    ///
    /// # Errors
    ///
    /// `Unavailable` when the request did not get through.
    pub async fn drawing(&self, image_model: Option<u32>) -> Result<reqwest::Response, GuiError> {
        let asked = image_model.map_or_else(String::new, |id| format!("?image_model={id}"));
        self.send(
            self.bounded
                .get(self.url(&format!("/images/drawing{asked}"))),
        )
        .await
    }
}

#[cfg(test)]
#[path = "far_images_tests.rs"]
mod far_images_tests;
