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
use gglib_app_services::FarChats;
use gglib_core::domain::hub_chats::HubTurn;
use serde::Deserialize;

use crate::error::HttpError;
use crate::state::AppState;

/// Body for `PUT /api/remote/chats/{id}/turns/{run_id}`: the new message
/// and nothing else. The far machine rebuilds the history from its record.
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub(crate) struct RemoteTurnBody {
    /// The user's message.
    pub content: String,
}

/// `?after=N` on a run's events.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
pub(crate) struct After {
    #[serde(default)]
    after: u32,
}

/// The far proxy, when this machine is joined to one and holds its key.
///
/// # Errors
///
/// `409` when not connected, or connected without a key: both are what
/// `gglib remote join` fixes, as the remote chat path says.
async fn far(state: &AppState) -> Result<FarChats, HttpError> {
    let Some(connection) = state.remote.status().await.connected else {
        return Err(HttpError::Conflict(
            "not connected to a remote machine — `gglib remote join` first".to_owned(),
        ));
    };
    let key = state
        .core
        .settings()
        .get()
        .await
        .map_err(|e| HttpError::Internal(format!("could not read settings: {e}")))?
        .remote_pairing
        .map(|stored| stored.api_key)
        .ok_or_else(|| {
            HttpError::Conflict(
                "connected to a remote machine, but this one holds no key for it — pair again \
                 with the full `<ticket>-<code>` string"
                    .to_owned(),
            )
        })?;
    Ok(FarChats::new(&connection.base_url, &key)?)
}

/// `GET /api/remote/chats`.
pub(crate) async fn list_chats(State(state): State<AppState>) -> Result<Response, HttpError> {
    list_chats_via(&far(&state).await?).await
}

/// `GET /api/remote/chats/{id}`.
pub(crate) async fn open_chat(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Response, HttpError> {
    open_chat_via(&far(&state).await?, id).await
}

/// `PUT /api/remote/chats/{id}/turns/{run_id}`.
pub(crate) async fn add_turn(
    State(state): State<AppState>,
    Path((id, run_id)): Path<(i64, String)>,
    Json(body): Json<RemoteTurnBody>,
) -> Result<Response, HttpError> {
    add_turn_via(&far(&state).await?, id, &run_id, body).await
}

/// `GET /api/remote/runs`.
pub(crate) async fn list_runs(State(state): State<AppState>) -> Result<Response, HttpError> {
    list_runs_via(&far(&state).await?).await
}

/// `POST /api/remote/runs/{run_id}/cancel`.
pub(crate) async fn cancel_run(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<Response, HttpError> {
    cancel_run_via(&far(&state).await?, &run_id).await
}

/// `GET /api/remote/runs/{run_id}/events?after=N`.
pub(crate) async fn run_events(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
    Query(After { after }): Query<After>,
) -> Result<Response, HttpError> {
    run_events_via(&far(&state).await?, &run_id, after).await
}

pub(super) async fn list_chats_via(far: &FarChats) -> Result<Response, HttpError> {
    Ok(relay(far.list_chats().await?).await)
}

pub(super) async fn open_chat_via(far: &FarChats, id: i64) -> Result<Response, HttpError> {
    Ok(relay(far.open_chat(id).await?).await)
}

pub(super) async fn add_turn_via(
    far: &FarChats,
    id: i64,
    run_id: &str,
    body: RemoteTurnBody,
) -> Result<Response, HttpError> {
    let turn = HubTurn {
        conversation_id: id,
        content: body.content,
    };
    Ok(relay(far.add_turn(run_id, &turn).await?).await)
}

pub(super) async fn list_runs_via(far: &FarChats) -> Result<Response, HttpError> {
    Ok(relay(far.list_runs().await?).await)
}

pub(super) async fn cancel_run_via(far: &FarChats, run_id: &str) -> Result<Response, HttpError> {
    Ok(relay(far.cancel_run(run_id).await?).await)
}

/// The events stream through as the far proxy sends them, never gathered
/// first: a reply is read while it is written.
pub(super) async fn run_events_via(
    far: &FarChats,
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

/// A far answer, handed back: a success as it came, a refusal in this
/// daemon's error shape with its status, its code and any `Retry-After`.
async fn relay(answer: reqwest::Response) -> Response {
    let status = answer.status();
    let retry_after = answer.headers().get(header::RETRY_AFTER).cloned();
    let content_type = answer.headers().get(header::CONTENT_TYPE).cloned();
    let Ok(bytes) = answer.bytes().await else {
        return HttpError::ServiceUnavailable("the other machine's answer was cut off".to_owned())
            .into_response();
    };
    let mut response = if status.is_success() {
        let mut ok = Response::new(Body::from(bytes));
        if let Some(value) = content_type {
            ok.headers_mut().insert(header::CONTENT_TYPE, value);
        }
        ok
    } else {
        refusal(status, &bytes).into_response()
    };
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
/// type}`, the shape the page reads. A refused key names the fix.
fn refusal(status: StatusCode, body: &[u8]) -> Json<serde_json::Value> {
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
    let far = serde_json::from_slice::<Far>(body).ok().map(|f| f.error);
    let (message, code) = if status == StatusCode::UNAUTHORIZED {
        (
            "the other machine is not admitting this device's key — pair again with a fresh \
             `gglib remote invite` there"
                .to_owned(),
            Some("key_refused".to_owned()),
        )
    } else {
        match far {
            Some(detail) => (detail.message, detail.code),
            None => (format!("the other machine answered {status}"), None),
        }
    };
    let shown = if status == StatusCode::UNAUTHORIZED {
        StatusCode::CONFLICT
    } else {
        status
    };
    let mut body = serde_json::json!({ "error": message, "status": shown.as_u16() });
    if let Some(code) = code {
        body["type"] = serde_json::Value::String(code);
    }
    Json(body)
}

#[cfg(test)]
#[path = "chats_tests.rs"]
mod chats_tests;
