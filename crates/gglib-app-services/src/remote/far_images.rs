//! What the far machine says about drawing.

use super::FarProxy;
use crate::error::GuiError;

impl FarProxy {
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
