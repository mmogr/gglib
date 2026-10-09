//! An MCP server's tool result, passed on to a `/mcp` client as MCP items.
//!
//! The gateway does not store anything. An `/mcp` client speaks MCP, so it
//! gets each of the server's content items as an item of its own: a text item
//! as text, an image item with its base64 bytes and `mimeType` as they came.
//! An item of any other kind becomes a text item naming it, without its
//! payload, in the words the agent's tool executor uses.

use gglib_core::McpToolResult;
use serde_json::Value;

use super::types::{CallToolResult, ToolContent};

/// `result` as the `tools/call` result an `/mcp` client reads.
///
/// A failure is its message as one text item, with `isError: true`.
pub(super) fn from_upstream(result: McpToolResult) -> CallToolResult {
    if !result.success {
        return CallToolResult::error(
            result
                .error
                .unwrap_or_else(|| "tool returned an error without a message".to_owned()),
        );
    }
    let content = match result.data {
        None => Vec::new(),
        Some(Value::String(text)) => vec![ToolContent::text(text)],
        Some(Value::Array(items)) => items.iter().map(item).collect(),
        Some(other) => vec![item(&other)],
    };
    CallToolResult {
        content,
        is_error: None,
    }
}

/// One content item: an image with its bytes and type, a text item as text,
/// anything else named.
fn item(value: &Value) -> ToolContent {
    let field = |name: &str| value.get(name).and_then(Value::as_str);
    if field("type") == Some("image")
        && let (Some(data), Some(mime_type)) = (field("data"), field("mimeType"))
    {
        return ToolContent::Image {
            data: data.to_owned(),
            mime_type: mime_type.to_owned(),
        };
    }
    ToolContent::text(field("text").map_or_else(|| gglib_mcp::describe_item(value), str::to_owned))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn wire(result: McpToolResult) -> Value {
        serde_json::to_value(from_upstream(result)).unwrap()
    }

    /// A text item is a text item, not the content array as JSON text.
    #[test]
    fn a_text_item_is_a_text_item() {
        let got = wire(McpToolResult::success(json!([
            { "type": "text", "text": "line 1" },
            { "type": "text", "text": "line 2" },
        ])));
        assert_eq!(
            got,
            json!({ "content": [
                { "type": "text", "text": "line 1" },
                { "type": "text", "text": "line 2" },
            ] })
        );
    }

    /// An image item keeps its bytes and its media type, in its place.
    #[test]
    fn an_image_item_is_passed_through_with_its_mime_type() {
        let got = wire(McpToolResult::success(json!([
            { "type": "text", "text": "A dot." },
            { "type": "image", "data": "iVBORw0KGgo=", "mimeType": "image/png" },
        ])));
        assert_eq!(
            got,
            json!({ "content": [
                { "type": "text", "text": "A dot." },
                { "type": "image", "data": "iVBORw0KGgo=", "mimeType": "image/png" },
            ] })
        );
    }

    /// A failure keeps `isError: true` and says why.
    #[test]
    fn an_error_sets_is_error_and_carries_its_message() {
        assert_eq!(
            wire(McpToolResult::error("boom")),
            json!({ "content": [{ "type": "text", "text": "boom" }], "isError": true })
        );
        let silent = McpToolResult {
            success: false,
            data: None,
            error: None,
        };
        assert_eq!(
            wire(silent)["content"][0]["text"],
            "tool returned an error without a message"
        );
    }

    /// Another kind of item, or an image without its data, is named in a
    /// text item; no payload is copied into it.
    #[test]
    fn another_item_is_named_not_dumped() {
        let got = from_upstream(McpToolResult::success(json!([
            { "type": "resource", "resource": { "uri": "file:///a.txt", "blob": "QUJD" } },
            { "type": "audio", "data": "UklGRg==", "mimeType": "audio/wav" },
            { "type": "image", "mimeType": "image/png" },
        ])));
        assert_eq!(
            got.content,
            vec![
                ToolContent::text("[resource file:///a.txt]"),
                ToolContent::text("[audio content not shown]"),
                ToolContent::text("[image content not shown]"),
            ]
        );
    }

    /// A plain string is one text item; no content is no items.
    #[test]
    fn a_plain_string_is_one_text_item() {
        let got = from_upstream(McpToolResult::success(json!("hello")));
        assert_eq!(got, CallToolResult::text("hello"));
        let empty = from_upstream(McpToolResult {
            success: true,
            data: None,
            error: None,
        });
        assert!(empty.content.is_empty());
        assert_eq!(empty.is_error, None);
    }
}
