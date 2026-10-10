//! Whether a message sent with Draw pressed can draw here: the one rule
//! behind the Draw button's availability, at every door that asks
//! (`GET /api/images/drawing`, the proxy's `GET /v1/images/drawing`, and a
//! run's `draw: true`).
//!
//! The reasons, in the order they are asked:
//!
//! 1. the chat's model is on another machine, whose loop would run its tools
//!    here while the image model is there;
//! 2. the chat's model calls no tools, so it could never call the tool;
//! 3. this process has no image driver (a proxy outside the daemon);
//! 4. what the driver answers ([`ImageGenerationPort::drawing_model`]): no
//!    image runtime, no image model, the default lacking a file or several
//!    and no default, the only one lacking a file.
//!
//! Every refusal is coded `drawing_unavailable`, whatever its reason: a client
//! greys the button and shows the words.

use crate::contracts::http::images::DrawingAvailability;
use crate::ports::ImageGenerationPort;

/// The code every refusal carries.
const UNAVAILABLE: &str = "drawing_unavailable";

/// Whether a message can draw: `far` when the chat's model is on another
/// machine, `calls_tools` `Some(false)` for a model that calls no tools,
/// `images` this process's image driver.
pub async fn drawing_availability(
    images: Option<&dyn ImageGenerationPort>,
    far: bool,
    calls_tools: Option<bool>,
) -> DrawingAvailability {
    if far {
        return DrawingAvailability::refused(
            UNAVAILABLE,
            "this chat's model is on another machine; a chat kept there draws with that \
             machine's image model",
        );
    }
    if calls_tools == Some(false) {
        return DrawingAvailability::refused(
            UNAVAILABLE,
            "this chat's model calls no tools, so it cannot draw; choose a model that calls tools",
        );
    }
    let Some(images) = images else {
        return DrawingAvailability::refused(
            UNAVAILABLE,
            "this proxy runs outside the gglib daemon, so it has nothing to draw with",
        );
    };
    match images.drawing_model().await {
        Ok(model) => DrawingAvailability::with(model),
        Err(e) => DrawingAvailability::refused(UNAVAILABLE, e.to_string()),
    }
}

#[cfg(test)]
#[path = "drawing_tests.rs"]
mod tests;
