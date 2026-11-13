//! The edits a forwarded frame takes, and nothing else.
//!
//! A streamed reply goes to the client as llama-server wrote it, frame for
//! frame (see [`crate::forward::drain_events`]). These are the only changes
//! the proxy makes to a frame on the way. Each says whether it changed
//! anything: a frame no edit changed is sent as the bytes that arrived, and
//! one that was changed is written again from its JSON, keys in alphabetical
//! order.

use bytes::Bytes;
use serde_json::{Value, json};

use gglib_core::domain::agent::ContextReading;

/// Name `model_name` in `frame`'s `model`, when the key is there and names
/// something else.
///
/// llama-server names its reply after the `-m` it was started with, since
/// gglib passes no `--alias`, so the frame as it comes tells a client where a
/// GGUF file sits on this machine rather than which model answered. The
/// non-streamed reply takes the same edit
/// ([`forward_unary`](crate::forward_unary)). A frame with no `model` is
/// left without one.
pub(crate) fn name_served_model(frame: &mut Value, model_name: &str) -> bool {
    match frame.get_mut("model") {
        Some(model) if *model != *model_name => {
            *model = model_name.into();
            true
        }
        _ => false,
    }
}

/// Add what `reading` knows to `frame`'s `usage` object, under the names a
/// `turn_usage` event spells them by. Only for a client that asked (see
/// [`crate::usage_reading`]); a reading that knows nothing adds no key.
pub(crate) fn add_reading(frame: &mut Value, reading: ContextReading) -> bool {
    let Ok(Value::Object(known)) = serde_json::to_value(reading) else {
        return false;
    };
    let Some(Value::Object(usage)) = frame.get_mut("usage") else {
        return false;
    };
    if known.is_empty() {
        return false;
    }
    usage.extend(known);
    true
}

/// Give `frame`'s `error` the code gglib relays for it: llama-server's when it
/// is a string, `upstream_error` otherwise, as the parsed event already says
/// (`message`, `error_type`, `code`).
///
/// llama-server writes a number there (the HTTP status), and a client such as
/// ggchat matches `code` against a closed list of strings. The rest of the
/// object, `n_ctx` and the like, is kept. An `error` that is not an object is
/// replaced by the event's three fields.
pub(crate) fn name_error_code(
    frame: &mut Value,
    message: &str,
    error_type: &str,
    code: &str,
) -> bool {
    let Some(error) = frame.get_mut("error") else {
        return false;
    };
    let relayed = json!({ "message": message, "type": error_type, "code": code });
    match error {
        Value::Object(fields) => {
            if fields.get("code") == relayed.get("code") {
                return false;
            }
            fields.insert("code".to_owned(), relayed["code"].clone());
        }
        other => *other = relayed,
    }
    true
}

/// Take the tool calls out of `frame`'s delta, for a finish frame that also
/// carried calls a repair has since replaced. Returns whether there were any.
pub(crate) fn drop_tool_calls(frame: &mut Value) -> bool {
    frame
        .pointer_mut("/choices/0/delta")
        .and_then(Value::as_object_mut)
        .is_some_and(|delta| delta.remove("tool_calls").is_some())
}

/// `frame` as one SSE `data:` block.
pub(crate) fn data_block(frame: &Value) -> Bytes {
    Bytes::from(format!("data: {frame}\n\n"))
}

/// A payload as it arrived, as one SSE `data:` block.
pub(crate) fn verbatim(payload: &str) -> Bytes {
    Bytes::from(format!("data: {payload}\n\n"))
}

#[cfg(test)]
#[path = "frame_edit_tests.rs"]
mod tests;
