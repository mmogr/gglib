//! `POST /v1/images/generations`: `OpenAI`'s Images API over the daemon's image
//! driver, and the same handler body at the daemon's
//! `POST /api/images/generations`, which `gglib image` calls.
//!
//! The body and the answer are `OpenAI`'s (`gglib_core::contracts::http::images`
//! says which fields, and what was checked against the SDKs): a prompt, an
//! optional model, one to four images, a `WIDTHxHEIGHT` size, gglib's seed,
//! base64 PNGs only. With gglib's `image` the picture it carries is drawn
//! from, at `strength`, and with `mask` only where the mask is white; a body
//! may be [`MAX_BODY_BYTES`](gglib_core::contracts::http::MAX_BODY_BYTES)
//! long to carry them. Without `stream` the answer is `{created, data:
//! [{b64_json}], output_format}` once the render is done, minutes later, with
//! no progress on the way. With `stream: true` the answer is server-sent
//! events (`images_stream.rs`): progress at every stage and step, partial
//! images up to the number asked for, then one completed event per image. A
//! client that leaves drops the render, which cancels its job; `sd-server`
//! cannot stop a generating job, so the driver stops `sd-server` itself and
//! the image model is loaded again for the next picture.
//!
//! The driver refuses what the image model cannot draw, before anything
//! queues; this file refuses only what no driver is asked: a body that does
//! not read, a size that is not `WIDTHxHEIGHT`, a format other than base64
//! PNG, more partial images than three.

use std::sync::Arc;

use axum::Json;
use axum::body::Bytes;
use axum::extract::rejection::BytesRejection;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use gglib_core::contracts::http::images::{
    DrawingQuery, ImageData, ImageGenerationsRequest, ImageGenerationsResponse, MAX_PARTIAL_IMAGES,
};
use gglib_core::ports::{
    DEFAULT_REDRAW_STRENGTH, ImageError, ImageGenerationPort, ImageRequest, ImageSize, ImageSource,
    MAX_IMAGES_PER_REQUEST,
};
use gglib_core::request_pipeline::image_size;
use gglib_core::services::drawing_availability;
use tokio::sync::mpsc;
use tracing::debug;

use crate::models::ErrorResponse;
use crate::remote::DeviceDrawing;
use crate::server::{AppState, handle_runtime_error};

/// `GET /v1/images/drawing`: whether a turn sent with Draw pressed can draw
/// here, and why not, by the same rule as the daemon's
/// `GET /api/images/drawing`. A paired device asks before it sends `draw`.
/// Beside it, `mcp_servers`: the MCP servers a device's run is offered.
///
/// Always 200. When it cannot, `code` is `drawing_unavailable`: not an
/// error of this request, but the code a request to draw would be refused
/// with (400), beside the reason.
pub(crate) async fn drawing_route(
    State(state): State<AppState>,
    Query(query): Query<DrawingQuery>,
) -> Json<DeviceDrawing> {
    let answer = drawing_availability(
        state.images.as_deref(),
        query.far,
        query.calls_tools,
        query.image_model,
    )
    .await;
    Json(DeviceDrawing::of(&state, answer))
}

/// The proxy's door: the driver the daemon handed this proxy, if any.
pub(crate) async fn generations_route(
    State(state): State<AppState>,
    body: Result<Bytes, BytesRejection>,
) -> Response {
    generations_read(state.images.clone(), body).await
}

/// [`generations`] for a body as a door's extractor read it: one over the
/// limit is refused with its code (`request_too_large`), at either door.
pub async fn generations_read(
    images: Option<Arc<dyn ImageGenerationPort>>,
    body: Result<Bytes, BytesRejection>,
) -> Response {
    match body {
        Ok(body) => generations(images, body).await,
        Err(rejection) => crate::body_limit::rejected(rejection),
    }
}

/// The handler body both doors share: read the request, draw, answer it as
/// JSON or as a stream. `images` is `None` for a proxy that runs outside the
/// daemon, which has nothing to draw with.
pub async fn generations(images: Option<Arc<dyn ImageGenerationPort>>, body: Bytes) -> Response {
    debug!("POST images/generations");
    let Some(images) = images else {
        return refuse(&ImageError::Unavailable {
            reason: "this proxy runs outside the gglib daemon, so it has nothing to draw with; \
                     send the request to the proxy the daemon runs"
                .to_owned(),
        });
    };
    let request: ImageGenerationsRequest = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::invalid_request(&format!(
                    "Invalid request body: {e}"
                ))),
            )
                .into_response();
        }
    };
    let ask = match read(&request) {
        Ok(ask) => ask,
        Err(refusal) => return *refusal,
    };
    if request.stream {
        crate::images_stream::respond(images, ask)
    } else {
        unary(images.as_ref(), ask).await
    }
}

/// A request read and checked: what to ask the driver, and what the stream
/// needs beside it.
pub(crate) struct Ask {
    pub(crate) request: ImageRequest,
    pub(crate) partial_images: u32,
    /// The size as asked, `WIDTHxHEIGHT`, or `auto`.
    pub(crate) size_label: String,
}

fn read(request: &ImageGenerationsRequest) -> Result<Ask, Box<Response>> {
    let invalid = |message: String| Box::new(refuse(&ImageError::Invalid { message }));
    if let Some(format) = request.response_format.as_deref()
        && format != "b64_json"
    {
        return Err(invalid(format!(
            "gglib answers images as b64_json only, not {format}; leave response_format out \
             or set it to b64_json"
        )));
    }
    if let Some(format) = request.output_format.as_deref()
        && format != "png"
    {
        return Err(invalid(format!(
            "gglib draws PNG only, not {format}; leave output_format out or set it to png"
        )));
    }
    let partial_images = request.partial_images.unwrap_or(0);
    if partial_images > MAX_PARTIAL_IMAGES {
        return Err(invalid(format!(
            "partial_images is from 0 to {MAX_PARTIAL_IMAGES}, not {partial_images}"
        )));
    }
    let n = match request.n {
        None => 1,
        Some(n) => u8::try_from(n)
            .ok()
            .filter(|n| (1..=MAX_IMAGES_PER_REQUEST).contains(n))
            .ok_or_else(|| {
                invalid(format!(
                    "n is from 1 to {MAX_IMAGES_PER_REQUEST} images, not {n}"
                ))
            })?,
    };
    let size = match request.size.as_deref() {
        None | Some("auto") => None,
        Some(text) => Some(text.parse::<ImageSize>().map_err(|e| {
            Box::new(
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse::with_code(
                        e.to_string(),
                        "invalid_request_error",
                        "invalid_image_size",
                    )),
                )
                    .into_response(),
            )
        })?),
    };
    let source = source_of(request).map_err(invalid)?;
    // A picture's size is the render's, so it is the size a stream names.
    let size_label = source
        .as_ref()
        .and_then(|source| image_size(&source.image))
        .map(|(width, height)| format!("{width}x{height}"))
        .or_else(|| request.size.clone())
        .unwrap_or_else(|| "auto".to_owned());
    Ok(Ask {
        request: ImageRequest {
            model: request.model.clone(),
            prompt: request.prompt.clone(),
            size,
            n,
            seed: request.seed,
            source,
        },
        partial_images,
        size_label,
    })
}

/// The picture a request draws from, with its mask and strength; `Err` with
/// what to put right for base64 that does not decode, and for a mask or a
/// strength sent with no picture. Whether the bytes are a PNG, and a size
/// the model draws, is the driver's to say.
fn source_of(request: &ImageGenerationsRequest) -> Result<Option<ImageSource>, String> {
    let Some(image) = request.image.as_deref() else {
        return match (&request.mask, request.strength) {
            (Some(_), _) => Err("mask goes with image: send the picture the mask is for".into()),
            (None, Some(_)) => {
                Err("strength goes with image: send the picture to draw from".into())
            }
            (None, None) => Ok(None),
        };
    };
    Ok(Some(ImageSource {
        image: base64_field("image", image)?,
        mask: request
            .mask
            .as_deref()
            .map(|mask| base64_field("mask", mask))
            .transpose()?,
        strength: request.strength.unwrap_or(DEFAULT_REDRAW_STRENGTH),
    }))
}

/// The bytes of `text`, base64 with or without a `data:` URL's prefix.
fn base64_field(name: &str, text: &str) -> Result<Vec<u8>, String> {
    let encoded = match text.split_once(";base64,") {
        Some((prefix, encoded)) if prefix.starts_with("data:") => encoded,
        _ => text,
    };
    BASE64
        .decode(encoded.trim())
        .map_err(|e| format!("{name} is not base64: {e}"))
}

async fn unary(images: &dyn ImageGenerationPort, ask: Ask) -> Response {
    // Nobody reads the reports of a render that does not stream; the driver
    // never waits on them.
    let (reports, _) = mpsc::channel(1);
    match images.generate(ask.request, reports).await {
        Ok(batch) => Json(ImageGenerationsResponse {
            created: crate::images::unix_now(),
            data: batch
                .images
                .iter()
                .map(|image| ImageData {
                    b64_json: BASE64.encode(&image.bytes),
                })
                .collect(),
            output_format: "png".to_owned(),
        })
        .into_response(),
        Err(error) => refuse(&error),
    }
}

/// Now, in Unix seconds, as `OpenAI`'s `created` fields count.
pub(crate) fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

/// The response refusing `error`: its status and its code, and for a runtime
/// error the proxy's own mapping, `Retry-After` included.
pub(crate) fn refuse(error: &ImageError) -> Response {
    if let ImageError::Runtime(e) = error {
        return handle_runtime_error(e.clone());
    }
    let status =
        StatusCode::from_u16(error.http_status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    (status, Json(refusal_body(error))).into_response()
}

/// The error body for `error`, as a response or a stream's last event
/// carries it.
pub(crate) fn refusal_body(error: &ImageError) -> ErrorResponse {
    if let ImageError::Runtime(e) = error {
        return ErrorResponse::from(e.clone());
    }
    let kind = match error.http_status() {
        400..=499 => "invalid_request_error",
        503 => "service_unavailable",
        _ => "server_error",
    };
    error.code().map_or_else(
        || ErrorResponse::new(error.to_string(), kind),
        |code| ErrorResponse::with_code(error.to_string(), kind, code),
    )
}

#[cfg(test)]
#[path = "images_tests.rs"]
mod tests;
