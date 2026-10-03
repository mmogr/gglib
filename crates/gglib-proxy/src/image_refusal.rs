//! The proxy's refusal of an image for a model that cannot read one.
//!
//! The rule, the code and the words are core's
//! ([`gglib_core::request_pipeline::refuse_unless_can_see`]); this is the
//! proxy's answer in its own error shape. It runs before admission, beside
//! the embedding refusal and for the same reason: forwarded, the request
//! would load the model, evicting whatever is serving, to collect
//! llama-server's HTTP 500 "image input is not supported".

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use gglib_core::ports::ModelSummary;
use gglib_core::request_pipeline::{has_images, refuse_unless_can_see};
use tracing::info;

use crate::models::ErrorResponse;

/// The refusal of `body` for `model`, or `None` when it may go on: `model`
/// reads images, or no message of `body`, history included, carries one. A
/// body that is not JSON carries none.
pub(crate) fn refuse_images(model: &ModelSummary, body: &Bytes) -> Option<Response> {
    let has_images = serde_json::from_slice(body).is_ok_and(|body| has_images(&body));
    let refusal = refuse_unless_can_see(model.image_input, has_images).err()?;
    info!(
        model = %model.name,
        "refusing a chat completion with images for a model with no projector"
    );
    Some(
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::with_code(
                refusal.message(&model.name),
                "invalid_request_error",
                refusal.code(),
            )),
        )
            .into_response(),
    )
}
