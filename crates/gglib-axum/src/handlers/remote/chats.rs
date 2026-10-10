//! The far machine's chats and runs, for this machine's chat page: each
//! route forwards to the far proxy through the tunnel with the stored key,
//! and hands the answer back.
//!
//! The browser never holds the key (ADR 0012, decision 7), so the page asks
//! here. Nothing is kept: a body passes through, and a run's events stream
//! through as they come. A far refusal keeps its status and its code, in
//! this daemon's error shape, but for a refused key: a `401` from here
//! would read to the page as this daemon wanting a key of its own.

use axum::Json;
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use gglib_app_services::FarProxy;
use gglib_core::domain::hub_chats::HubTurn;
use gglib_core::domain::{AttachmentId, Thinking};
use serde::Deserialize;

use crate::error::HttpError;
use crate::state::AppState;

/// Body for `PUT /api/remote/chats/{id}/turns/{run_id}`: the new message,
/// and the chat's Thinking choice on the turn that changes it. The far
/// machine rebuilds the history from its record, so a body that carries
/// more is refused rather than half read. An image is named by the id
/// `POST /api/remote/attachments` answered.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub(crate) struct RemoteTurnBody {
    /// The user's message. Empty when the turn is its images alone.
    pub content: String,
    /// The images the message carries, by id, in order. Left out of the
    /// body when there are none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "ts-bindings", ts(type = "Array<string>", optional))]
    pub images: Vec<AttachmentId>,
    /// The far chat's Thinking choice, as [`HubTurn::thinking`] carries it:
    /// said only on the turn that changes it, and remembered there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    pub thinking: Option<Thinking>,
}

/// `?after=N` on a run's events.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
pub(crate) struct After {
    #[serde(default)]
    after: u32,
}

/// `GET /api/remote/chats`.
pub(crate) async fn list_chats(State(state): State<AppState>) -> Result<Response, HttpError> {
    list_chats_via(&state.remote.far().await?).await
}

/// `GET /api/remote/chats/{id}`.
pub(crate) async fn open_chat(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Response, HttpError> {
    open_chat_via(&state.remote.far().await?, id).await
}

/// `PUT /api/remote/chats/{id}/turns/{run_id}`.
pub(crate) async fn add_turn(
    State(state): State<AppState>,
    Path((id, run_id)): Path<(i64, String)>,
    Json(body): Json<RemoteTurnBody>,
) -> Result<Response, HttpError> {
    add_turn_via(&state.remote.far().await?, id, &run_id, body).await
}

/// `GET /api/remote/runs`.
pub(crate) async fn list_runs(State(state): State<AppState>) -> Result<Response, HttpError> {
    list_runs_via(&state.remote.far().await?).await
}

/// `POST /api/remote/runs/{run_id}/cancel`.
pub(crate) async fn cancel_run(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<Response, HttpError> {
    cancel_run_via(&state.remote.far().await?, &run_id).await
}

/// `GET /api/remote/runs/{run_id}/events?after=N`.
pub(crate) async fn run_events(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
    Query(After { after }): Query<After>,
) -> Result<Response, HttpError> {
    run_events_via(&state.remote.far().await?, &run_id, after).await
}

pub(super) async fn list_chats_via(far: &FarProxy) -> Result<Response, HttpError> {
    Ok(relay(far.list_chats().await?).await)
}

pub(super) async fn open_chat_via(far: &FarProxy, id: i64) -> Result<Response, HttpError> {
    Ok(relay(far.open_chat(id).await?).await)
}

pub(super) async fn add_turn_via(
    far: &FarProxy,
    id: i64,
    run_id: &str,
    body: RemoteTurnBody,
) -> Result<Response, HttpError> {
    let turn = HubTurn {
        conversation_id: id,
        content: body.content,
        images: body.images,
        thinking: body.thinking,
        answer_saved: false,
    };
    Ok(relay(far.add_turn(run_id, &turn).await?).await)
}

pub(super) async fn list_runs_via(far: &FarProxy) -> Result<Response, HttpError> {
    Ok(relay(far.list_runs().await?).await)
}

pub(super) async fn cancel_run_via(far: &FarProxy, run_id: &str) -> Result<Response, HttpError> {
    Ok(relay(far.cancel_run(run_id).await?).await)
}

/// The events stream through as the far proxy sends them, never gathered
/// first: a reply is read while it is written.
pub(super) async fn run_events_via(
    far: &FarProxy,
    run_id: &str,
    after: u32,
) -> Result<Response, HttpError> {
    let answer = far.run_events(run_id, after).await?;
    if !answer.status().is_success() {
        return Ok(relay(answer).await);
    }
    let status = answer.status();
    let mut response = Response::new(Body::from_stream(answer.bytes_stream()));
    *response.status_mut() = status;
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/event-stream"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    Ok(response)
}

/// A far answer, handed back: a success as it came, a refusal as
/// [`refused`] gives it.
pub(super) async fn relay(answer: reqwest::Response) -> Response {
    let status = answer.status();
    let retry_after = answer.headers().get(header::RETRY_AFTER).cloned();
    let content_type = answer.headers().get(header::CONTENT_TYPE).cloned();
    let Ok(bytes) = answer.bytes().await else {
        return HttpError::ServiceUnavailable("the other machine's answer was cut off".to_owned())
            .into_response();
    };
    if !status.is_success() {
        return refused(status, retry_after, &bytes);
    }
    let mut ok = Response::new(Body::from(bytes));
    *ok.status_mut() = status;
    if let Some(value) = content_type {
        ok.headers_mut().insert(header::CONTENT_TYPE, value);
    }
    if let Some(value) = retry_after {
        ok.headers_mut().insert(header::RETRY_AFTER, value);
    }
    ok
}

/// A far refusal in this daemon's error shape, with its status, its code and
/// any `Retry-After` — but for a refused key, which is a `409`.
pub(super) fn refused(
    status: StatusCode,
    retry_after: Option<HeaderValue>,
    body: &[u8],
) -> Response {
    let mut response = refusal(status, body).into_response();
    *response.status_mut() = if status == StatusCode::UNAUTHORIZED {
        StatusCode::CONFLICT
    } else {
        status
    };
    if let Some(value) = retry_after {
        response.headers_mut().insert(header::RETRY_AFTER, value);
    }
    response
}

/// The far proxy's `{"error": {"message", "code"}}` as `{error, status,
/// type}`, the shape the page reads.
fn refusal(status: StatusCode, body: &[u8]) -> Json<serde_json::Value> {
    let (shown, message, code) = refusal_parts(status, body);
    let mut body = serde_json::json!({ "error": message, "status": shown.as_u16() });
    if let Some(code) = code {
        body["type"] = serde_json::Value::String(code);
    }
    Json(body)
}

/// A far refusal as this daemon says it: the status it answers with, the
/// message and the code. A refused key is a `409` that names the fix.
pub(super) fn refusal_parts(
    status: StatusCode,
    body: &[u8],
) -> (StatusCode, String, Option<String>) {
    #[derive(Deserialize)]
    struct Detail {
        message: String,
        #[serde(default)]
        code: Option<String>,
    }
    #[derive(Deserialize)]
    struct Far {
        error: Detail,
    }
    if status == StatusCode::UNAUTHORIZED {
        return (
            StatusCode::CONFLICT,
            "the other machine is not admitting this device's key — pair again with a fresh \
             `gglib remote invite` there"
                .to_owned(),
            Some("key_refused".to_owned()),
        );
    }
    match serde_json::from_slice::<Far>(body) {
        Ok(Far { error }) => (status, error.message, error.code),
        Err(_) => (status, format!("the other machine answered {status}"), None),
    }
}

#[cfg(test)]
#[path = "chats_tests.rs"]
mod chats_tests;
