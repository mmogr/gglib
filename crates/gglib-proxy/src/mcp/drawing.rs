//! `builtin__generate_image`: an `/mcp` client draws with this machine's
//! image model, when the person has switched that on.
//!
//! The tool is in the gateway's index, and can be invoked, only while the
//! `mcp_drawing` setting is on (it is off unless set) and drawing is available
//! here ([`drawing_availability`]: a daemon's image driver, an image runtime,
//! one image model to draw with). Otherwise it is in no list, and an invoke
//! that names it anyway is refused with the reason, before anything is
//! drawn.
//!
//! The answer is the image itself: one MCP `image` item per image drawn, then
//! one text item saying what was drawn. Nothing is stored, because nothing
//! here would ever link to it. A render takes minutes, so a request that
//! carried `_meta.progressToken` gets `notifications/progress` frames on its
//! SSE response while it waits ([`Meter`]); one that carried none gets only
//! the answer.
//!
//! `builtin` is not an MCP server: [`gglib_mcp::McpService`] refuses that
//! name to a server, so the id cannot name anyone else's tool.

use std::convert::Infallible;
use std::sync::Arc;

use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use futures_util::{Stream, StreamExt as _};
use gglib_core::McpTool;
use gglib_core::ports::{
    ImageBatch, ImageError, ImageGenerationPort, ImageRequest, ImageStage, MAX_IMAGES_PER_REQUEST,
};
use gglib_core::services::drawing_availability;
use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::server::AppState;

use super::types::{CallToolResult, INVALID_PARAMS, JsonRpcError, JsonRpcResponse, ToolContent};

/// The tool's id in the gateway's index: the reserved server name, the `__`
/// every id is split on, the tool.
pub(super) const TOOL_ID: &str = "builtin__generate_image";

/// What an invoke is told while the switch is off.
const SWITCHED_OFF: &str = "drawing through /mcp is switched off; turn it on in Settings, or run \
                            `gglib config settings set --mcp-drawing true`";

/// The image driver to draw with, when the switch is on and drawing is
/// available; otherwise why the tool is not offered.
pub(super) async fn offered(state: &AppState) -> Result<Arc<dyn ImageGenerationPort>, String> {
    if !state.settings.get().await.effective_mcp_drawing() {
        return Err(SWITCHED_OFF.to_owned());
    }
    // The client's model is calling tools and is not this machine's to place,
    // so neither of the chat-side reasons is asked.
    let availability = drawing_availability(state.images.as_deref(), false, None).await;
    match &state.images {
        Some(images) if availability.available => Ok(Arc::clone(images)),
        _ => Err(availability
            .reason
            .unwrap_or_else(|| "nothing here can draw".to_owned())),
    }
}

/// The tool as the index lists it and `get_tool_schema` describes it.
pub(super) fn definition() -> McpTool {
    McpTool::new("generate_image")
        .with_description(
            "Draw an image with this machine's image model. Write a detailed prompt. A render \
             takes from one to several minutes; the answer is the image and one sentence about it.",
        )
        .with_input_schema(json!({
            "type": "object",
            "properties": {
                "prompt": {
                    "type": "string",
                    "description": "A detailed description of the picture: subject, setting, \
                                    style, lighting, composition."
                },
                "size": {
                    "type": "string",
                    "description": "WIDTHxHEIGHT, such as 1024x1024; leave it out for the \
                                    model's default square."
                },
                "n": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": MAX_IMAGES_PER_REQUEST,
                    "description": "How many images; leave it out for one."
                },
                "seed": {
                    "type": "integer",
                    "description": "A seed, to draw the same picture again; leave it out for \
                                    a random one."
                }
            },
            "required": ["prompt"]
        }))
}

/// Answer an `invoke_tool` that names [`TOOL_ID`]: refused when the tool is
/// not offered, else drawn, with progress for `token` when the request
/// carried one.
pub(super) async fn call(
    state: &AppState,
    id: Value,
    arguments: Option<&Value>,
    token: Option<Value>,
) -> Response {
    let images = match offered(state).await {
        Ok(images) => images,
        Err(why) => {
            let error =
                JsonRpcError::new(INVALID_PARAMS, format!("Unknown tool: '{TOOL_ID}'. {why}"));
            return (
                StatusCode::OK,
                axum::Json(JsonRpcResponse::error(id, error)),
            )
                .into_response();
        }
    };
    // One set of argument rules for the tool, whichever door calls it.
    let events = match gglib_mcp::image_request_of(arguments.unwrap_or(&Value::Null)) {
        Ok(request) => render(id, images, request, token).left_stream(),
        // The caller's model can put this right, so it is a result it reads.
        Err(why) => futures_util::stream::once(async move {
            message(&answer_frame(id, CallToolResult::error(why)))
        })
        .right_stream(),
    };
    Sse::new(events.map(Ok::<_, Infallible>))
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// The token a `tools/call` asked to hear progress under: `_meta.progressToken`,
/// a string or a number.
pub(super) fn progress_token(params: &Value) -> Option<Value> {
    params
        .get("_meta")
        .and_then(|meta| meta.get("progressToken"))
        .filter(|token| token.is_string() || token.is_number())
        .cloned()
}

/// The render as the SSE response's events: a progress frame per report when
/// there is a token, then the answer. The render runs inside the stream, so
/// a client that leaves drops it.
fn render(
    id: Value,
    images: Arc<dyn ImageGenerationPort>,
    request: ImageRequest,
    token: Option<Value>,
) -> impl Stream<Item = Event> {
    async_stream::stream! {
        let (reports, mut rx) = mpsc::channel(64);
        let mut meter = Meter::new(u32::from(request.n));
        let work = images.generate(request, reports);
        tokio::pin!(work);
        let result = loop {
            tokio::select! {
                biased;
                Some(progress) = rx.recv() => {
                    if let Some(frame) = progress_frame(token.as_ref(), &mut meter, &progress.stage) {
                        yield message(&frame);
                    }
                }
                result = &mut work => break result,
            }
        };
        while let Ok(progress) = rx.try_recv() {
            if let Some(frame) = progress_frame(token.as_ref(), &mut meter, &progress.stage) {
                yield message(&frame);
            }
        }
        yield message(&answer_frame(id, answer(result)));
    }
}

/// One JSON-RPC message as an SSE `message` event.
fn message(frame: &Value) -> Event {
    Event::default().event("message").data(frame.to_string())
}

/// The response that ends the call.
fn answer_frame(id: Value, result: CallToolResult) -> Value {
    serde_json::to_value(JsonRpcResponse::success(
        id,
        serde_json::to_value(result).unwrap_or_default(),
    ))
    .unwrap_or_default()
}

/// What was drawn, as the call's result: each image inline, then the
/// sentence. A refusal or a failed render is its message, with `isError`.
fn answer(result: Result<ImageBatch, ImageError>) -> CallToolResult {
    match result {
        Ok(batch) => {
            let mut content: Vec<ToolContent> = batch
                .images
                .iter()
                .map(|image| ToolContent::Image {
                    data: BASE64.encode(&image.bytes),
                    mime_type: image.mime.to_owned(),
                })
                .collect();
            content.push(ToolContent::text(drew_sentence(&batch)));
            CallToolResult {
                content,
                is_error: None,
            }
        }
        Err(error) => CallToolResult::error(error.to_string()),
    }
}

/// "Drew 1 image, 1024x1024 PNG, with flux in 76 s; it is attached to this
/// result, and gglib kept no copy."
fn drew_sentence(batch: &ImageBatch) -> String {
    let drawn = batch.images.len();
    let (plural, attached) = if drawn == 1 {
        ("image", "it is")
    } else {
        ("images", "they are")
    };
    let size = batch.images.first().map_or_else(String::new, |image| {
        let format = image
            .mime
            .strip_prefix("image/")
            .unwrap_or(image.mime)
            .to_uppercase();
        format!(", {}x{} {format}", image.width, image.height)
    });
    let secs = batch.elapsed.as_secs_f64().round();
    format!(
        "Drew {drawn} {plural}{size}, with {} in {secs:.0} s; {attached} attached to this \
         result, and gglib kept no copy.",
        batch.model
    )
}

/// The `notifications/progress` frame for a report, when the caller gave a
/// token and the report moves the count on.
fn progress_frame(token: Option<&Value>, meter: &mut Meter, stage: &ImageStage) -> Option<Value> {
    let token = token?;
    let (progress, total) = meter.count(stage)?;
    let mut params = json!({
        "progressToken": token,
        "progress": progress,
        "message": words(stage, meter.passes),
    });
    if let Some(total) = total {
        params["total"] = json!(total);
    }
    Some(json!({"jsonrpc": "2.0", "method": "notifications/progress", "params": params}))
}

/// What a stage is called in a progress frame's `message`.
fn words(stage: &ImageStage, passes: u32) -> String {
    match stage {
        ImageStage::Queued { position, behind } => behind.as_ref().map_or_else(
            || format!("queued, place {position} in line"),
            |behind| format!("queued, place {position} in line behind {behind}"),
        ),
        ImageStage::Loading => "loading the image model".to_owned(),
        ImageStage::Sampling { pass, step, total } => {
            format!("sampling step {step} of {total}, image {pass} of {passes}")
        }
        ImageStage::Decoding => "decoding".to_owned(),
        ImageStage::Finishing => "finishing".to_owned(),
    }
}

/// A render's reports as MCP's progress: a count that only rises, and its
/// total once the render says how many steps it takes.
///
/// MCP requires each notification's `progress` to be more than the last, and
/// a render reports things that are not steps (its place in line, loading,
/// decoding). So the unit is one report: each wait before the first step
/// counts one, each step counts one, decoding and finishing one each. The
/// total is unknown, and left out, until the first step.
#[derive(Debug)]
struct Meter {
    /// How many images the request asked for: one pass each.
    passes: u32,
    /// The last count sent.
    last: u64,
    /// Once the first step has come: the count before it, and every pass's
    /// steps together.
    sampling: Option<(u64, u64)>,
}

impl Meter {
    const fn new(passes: u32) -> Self {
        Self {
            passes,
            last: 0,
            sampling: None,
        }
    }

    /// The `(progress, total)` to send for `stage`, or `None` for a report
    /// that would not raise the count.
    fn count(&mut self, stage: &ImageStage) -> Option<(u64, Option<u64>)> {
        let next = match (stage, self.sampling) {
            (ImageStage::Sampling { pass, step, total }, started) => {
                let (pass, step, total) = (u64::from(*pass), u64::from(*step), u64::from(*total));
                let (before, _) = *self.sampling.insert(
                    started
                        .unwrap_or_else(|| (self.last, u64::from(self.passes).max(pass) * total)),
                );
                before + pass.saturating_sub(1) * total + step
            }
            // A wait once steps have begun (the next pass loading) is inside
            // the steps' count already.
            (ImageStage::Queued { .. } | ImageStage::Loading, Some(_)) => return None,
            (ImageStage::Decoding, Some((before, steps))) => before + steps + 1,
            (ImageStage::Finishing, Some((before, steps))) => before + steps + 2,
            // Before any step, each report is one more.
            (_, None) => self.last + 1,
        };
        if next <= self.last {
            return None;
        }
        self.last = next;
        Some((
            next,
            self.sampling.map(|(before, steps)| before + steps + 2),
        ))
    }
}

#[cfg(test)]
#[path = "drawing_tests.rs"]
mod tests;
