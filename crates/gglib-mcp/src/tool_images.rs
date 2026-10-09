//! An MCP tool's `content` array, split into the text the model reads and
//! the images the conversation keeps.
//!
//! A `tools/call` result is a list of items: `text`, `image` (base64 `data`
//! and a `mimeType`), and others such as `resource`. Text items join with
//! `\n`. Each image is decoded and stored through
//! [`AttachmentService::ingest`], and the text says in one sentence what
//! became of it, in its place in the list. No image's bytes are ever part
//! of the text.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use gglib_core::domain::AttachmentInfo;
use gglib_core::ports::AttachmentError;
use gglib_core::services::AttachmentService;
use serde_json::Value;

/// A tool result's content as the model reads it, and the images it made.
#[derive(Debug)]
pub(crate) struct ToolContent {
    /// The text items, and a sentence for each image, joined with `\n`.
    pub(crate) text: String,
    /// The images that were stored, in the order they came.
    pub(crate) images: Vec<AttachmentInfo>,
}

/// Split `content`, storing its images through `attachments`.
///
/// A plain string is the text as it is. Items with a `text` string join
/// with `\n`; an `image` item adds `[image WxH PNG stored]`, or
/// `[image not stored: <why>]` when it is refused, and nothing is stored
/// then. When that leaves no text, each item is named by its kind instead
/// (`[resource <uri>]`, `[audio content not shown]`), so the model learns
/// what came back without reading its bytes; a number, `true`, `false` or
/// `null` is its JSON text, and an empty list is `[no content]`.
pub(crate) async fn split_content(content: &Value, attachments: &AttachmentService) -> ToolContent {
    if let Some(text) = content.as_str() {
        return ToolContent {
            text: text.to_owned(),
            images: Vec::new(),
        };
    }
    let items = content
        .as_array()
        .map_or_else(|| std::slice::from_ref(content), Vec::as_slice);

    let mut lines = Vec::new();
    let mut images = Vec::new();
    for item in items {
        if let Some(text) = item.get("text").and_then(Value::as_str) {
            lines.push(text.to_owned());
        } else if item.get("type").and_then(Value::as_str) == Some("image") {
            match store_image(item, attachments).await {
                Ok(info) => {
                    lines.push(stored_sentence(&info));
                    images.push(info);
                }
                Err(why) => lines.push(format!("[image not stored: {why}]")),
            }
        }
    }
    if lines.is_empty() {
        lines = items.iter().map(describe).collect();
    }
    if lines.is_empty() {
        lines.push("[no content]".to_owned());
    }
    ToolContent {
        text: lines.join("\n"),
        images,
    }
}

/// Decode an `image` item's `data` and store it, or say why not. A store
/// fault is also logged, without the bytes, since it is the operator's to
/// fix rather than the tool's.
async fn store_image(
    item: &Value,
    attachments: &AttachmentService,
) -> Result<AttachmentInfo, String> {
    let bytes = item
        .get("data")
        .and_then(Value::as_str)
        .and_then(|data| STANDARD.decode(data).ok())
        .ok_or_else(|| "The data is not base64.".to_owned())?;
    attachments
        .ingest(&bytes)
        .await
        .map(|upload| upload.info)
        .map_err(|refusal| {
            if let AttachmentError::Storage(_) = refusal {
                tracing::warn!(error = %refusal, "A tool's image could not be stored");
            }
            refusal.to_string()
        })
}

/// `[image 1024x1024 PNG stored]`: the size and format of a stored image,
/// in the shape of the CLI's image marker.
fn stored_sentence(info: &AttachmentInfo) -> String {
    let format = info
        .mime
        .strip_prefix("image/")
        .unwrap_or(&info.mime)
        .to_uppercase();
    format!("[image {}x{} {format} stored]", info.width, info.height)
}

/// A line naming an item that is neither text nor an image, without its
/// payload: an embedded resource keeps its URI and any text it holds. A
/// number, a boolean or `null` holds no payload, so it is its JSON text.
fn describe(item: &Value) -> String {
    if matches!(item, Value::Number(_) | Value::Bool(_) | Value::Null) {
        return item.to_string();
    }
    let kind = item.get("type").and_then(Value::as_str);
    match kind {
        Some("resource") => {
            let resource = item.get("resource");
            let uri = resource
                .and_then(|r| r.get("uri"))
                .and_then(Value::as_str)
                .unwrap_or("without a uri");
            let text = resource.and_then(|r| r.get("text")).and_then(Value::as_str);
            text.map_or_else(
                || format!("[resource {uri}]"),
                |text| format!("[resource {uri}]\n{text}"),
            )
        }
        Some("resource_link") => {
            let uri = item
                .get("uri")
                .and_then(Value::as_str)
                .unwrap_or("without a uri");
            format!("[resource link {uri}]")
        }
        Some(kind) => format!("[{kind} content not shown]"),
        None => "[content not shown]".to_owned(),
    }
}

#[cfg(test)]
#[path = "tool_images_tests.rs"]
pub(crate) mod tool_images_tests;
