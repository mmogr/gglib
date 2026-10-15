//! The answer the loop guard gives in the model's place when a noted run
//! trips again (ADR 0021).
//!
//! Under `note` the first trip in a run forwards the request with a note. If
//! the same run trips on a later request, the model kept repeating after it
//! was told, and another note would buy another generation of the same. So
//! the proxy does not forward that request: it answers it with a finished
//! assistant message that calls no tool, which is the turn every agentic
//! client seen so far ends its loop on. The status is 200, because the turn
//! did finish, and a refusal is what `note` exists to avoid.
//!
//! The text is fixed and carries the note's marker, so the person reading the
//! chat can tell gglib's words from the model's. Nothing from the request is
//! echoed in it.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::forward::finished_turn_body;
use crate::loop_guard_note::MARKER;
use crate::models::{ChatChoice, ChatCompletionResponse, ChatMessage};

/// The sentence the run ends with, marker included.
pub(crate) fn ended_text() -> String {
    format!(
        "{MARKER} The model repeated the same step after being told to change \
         approach, so gglib's loop guard stopped this run."
    )
}

/// The response that ends the run: one finished assistant message carrying
/// [`ended_text`], `finish_reason: "stop"` and no tool call, streamed when the
/// client asked for a stream.
pub(crate) fn ended_response(model: &str, streaming: bool) -> Response {
    if streaming {
        return Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "text/event-stream")
            .header("cache-control", "no-cache")
            .body(axum::body::Body::from(finished_turn_body(
                model,
                &ended_text(),
            )))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
    }
    let created = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
    let body = ChatCompletionResponse {
        id: format!("chatcmpl-{}", uuid::Uuid::new_v4().simple()),
        object: "chat.completion".to_owned(),
        created,
        model: model.to_owned(),
        choices: vec![ChatChoice {
            index: 0,
            message: ChatMessage {
                role: "assistant".to_owned(),
                content: Some(ended_text()),
                tool_calls: None,
                tool_call_id: None,
            },
            finish_reason: Some("stop".to_owned()),
        }],
        usage: None,
    };
    (StatusCode::OK, Json(body)).into_response()
}
