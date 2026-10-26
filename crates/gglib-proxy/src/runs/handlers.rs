//! `/v1/runs/*`: the same five calls as the daemon's `/api/runs/*`, in the
//! caller's scope.
//!
//! Every error message here is fixed text or a [`RunsError`]'s, which is
//! fixed text too: nothing echoes a request body or a frame.

use std::sync::Arc;

use axum::Json;
use axum::extract::rejection::{JsonRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use gglib_core::DeviceServers;
use gglib_core::domain::runs::RunKind;
use gglib_core::ports::{RunsError, RunsPort};
use serde::Deserialize;
use serde_json::Value;

use super::scope::Caller;
use super::sse;
use crate::models::ErrorResponse;
use crate::server::AppState;

pub(super) type Answer = Result<Response, Response>;

pub(super) fn error(status: StatusCode, error_type: &str, code: &str, message: String) -> Response {
    (
        status,
        Json(ErrorResponse::with_code(message, error_type, code)),
    )
        .into_response()
}

/// A refusal from the runs, with its own status and code.
fn refused(err: &RunsError) -> Response {
    let status = StatusCode::from_u16(err.http_status()).unwrap_or(StatusCode::BAD_REQUEST);
    let error_type = if status == StatusCode::TOO_MANY_REQUESTS {
        "rate_limit_error"
    } else {
        "invalid_request_error"
    };
    error(status, error_type, err.code(), err.to_string())
}

/// A request the route could not read.
pub(super) fn invalid(message: &str) -> Response {
    error(
        StatusCode::BAD_REQUEST,
        "invalid_request_error",
        "invalid_request",
        message.to_owned(),
    )
}

/// The answer when this proxy was started without runs.
pub(super) fn unavailable() -> Response {
    error(
        StatusCode::SERVICE_UNAVAILABLE,
        "service_unavailable",
        "runs_unavailable",
        "this proxy is not running under the gglib daemon, so it holds no runs".to_owned(),
    )
}

/// The runs, or `None` when this proxy was started without them.
fn runs(state: &AppState) -> Option<Arc<dyn RunsPort>> {
    state.runs.clone()
}

#[derive(Deserialize)]
pub(crate) struct PutQuery {
    /// `agent` adds a turn to a hub chat; absent, a chat run.
    #[serde(default)]
    kind: Option<RunKind>,
    /// `builtin` runs a chat run through the agent loop, for a chat the
    /// device keeps itself, offered the tools of the MCP servers open to
    /// paired devices (`gglib mcp allow-remote <server>`); `none` runs it
    /// the same way offered none of them, for such a chat with its tools
    /// off; absent, a plain chat run, recorded as it always was.
    #[serde(default)]
    tools: Option<Tools>,
    /// With `tools=builtin` or `tools=none`: the message was sent with Draw
    /// pressed, so the image tool is offered. Absent or `false`, it is not.
    /// Refused with `kind=agent`, whose turn says `draw` in its body.
    #[serde(default)]
    draw: bool,
    /// With `draw=true` beside `tools=builtin` or `tools=none`: the image
    /// model the person chose for the picture, by its id in
    /// `GET /v1/images/drawing`'s `models`. Absent, the default rule draws.
    /// Refused with `kind=agent`, whose turn says `image_model` in its
    /// body, and on a plain chat run.
    #[serde(default)]
    image_model: Option<u32>,
}

/// What `?tools=` may say. Any other word is refused, so a gglib that
/// predates a word refuses it rather than running the tools it would keep
/// out.
#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
enum Tools {
    /// Through the agent loop, offered the opened servers' tools.
    Builtin,
    /// Through the agent loop, offered no MCP tool: only the image tool
    /// with `draw`, and no tool at all without it.
    None,
}

/// `PUT /v1/runs/{id}`: start a chat run with the body as its request, or
/// answer with the caller's run that already has the id. 201 new, 200
/// existing. With `?kind=agent`, a device's turn on a hub chat, and with
/// `?tools=builtin` a chat run through the agent loop with this machine's
/// tools, or `?tools=none` one without its MCP tools: see [`super::turn`].
pub(crate) async fn put_run(
    State(state): State<AppState>,
    Caller(scope): Caller,
    Path(id): Path<String>,
    query: Result<Query<PutQuery>, QueryRejection>,
    body: Result<Json<Value>, JsonRejection>,
) -> Answer {
    let Ok(Query(query)) = query else {
        return Err(invalid(
            "`kind` is `chat` or `agent`, `tools` is `builtin` or `none`, `draw` is `true` or \
             `false`, and `image_model` is an image model's id",
        ));
    };
    let body = match body {
        Ok(Json(body)) => body,
        Err(rejection) => {
            return Err(
                crate::body_limit::too_large(rejection.status()).unwrap_or_else(|| {
                    invalid("a run's request body must be a JSON object, sent as application/json")
                }),
            );
        }
    };
    if query.kind == Some(RunKind::Agent) {
        if query.draw || query.image_model.is_some() {
            return Err(invalid(super::AGENT_DRAW_IN_QUERY));
        }
        // Refused, not ignored: a `tools=none` dropped here would run a Mac
        // chat's tools against the word the device sent.
        if query.tools.is_some() {
            return Err(invalid(
                "a turn on a hub chat takes its tools from the chat's settings; `tools` goes with `kind=chat`",
            ));
        }
        return super::turn::put(&state, scope, &id, body).await;
    }
    if let Some(tools) = query.tools {
        // An `image_model` without `draw` is the starter's to refuse, with
        // every other door's.
        let drawing = (query.draw, query.image_model);
        let servers = match tools {
            Tools::Builtin => DeviceServers::All,
            Tools::None => DeviceServers::none(),
        };
        return super::turn::put_chat(&state, scope, &id, body, drawing, servers).await;
    }
    if query.draw || query.image_model.is_some() {
        return Err(invalid(
            "`draw` and `image_model` go with `tools=builtin` or `tools=none`: a plain chat run \
             calls no tool",
        ));
    }
    let runs = runs(&state).ok_or_else(unavailable)?;
    let created = runs.create(scope, &id, body).map_err(|e| refused(&e))?;
    let status = if created.created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, Json(created.info)).into_response())
}

/// `GET /v1/runs`: every run the caller may see, newest first.
pub(crate) async fn list_runs(State(state): State<AppState>, Caller(scope): Caller) -> Answer {
    Ok(Json(runs(&state).ok_or_else(unavailable)?.list(&scope)).into_response())
}

/// `GET /v1/runs/{id}`.
pub(crate) async fn get_run(
    State(state): State<AppState>,
    Caller(scope): Caller,
    Path(id): Path<String>,
) -> Answer {
    let info = runs(&state)
        .ok_or_else(unavailable)?
        .get(&scope, &id)
        .map_err(|e| refused(&e))?;
    Ok(Json(info).into_response())
}

/// `POST /v1/runs/{id}/cancel`: idempotent.
pub(crate) async fn cancel_run(
    State(state): State<AppState>,
    Caller(scope): Caller,
    Path(id): Path<String>,
) -> Answer {
    let info = runs(&state)
        .ok_or_else(unavailable)?
        .cancel(&scope, &id)
        .map_err(|e| refused(&e))?;
    Ok(Json(info).into_response())
}

#[derive(Deserialize)]
pub(crate) struct EventsQuery {
    /// The last event the client has; 0 for all of them.
    #[serde(default)]
    after: u32,
}

/// `GET /v1/runs/{id}/events?after=N`: see [`sse`]. Ends when the proxy
/// shuts down, so a graceful stop does not wait on a run still going.
pub(crate) async fn run_events(
    State(state): State<AppState>,
    Caller(scope): Caller,
    Path(id): Path<String>,
    query: Result<Query<EventsQuery>, QueryRejection>,
) -> Answer {
    let runs = runs(&state).ok_or_else(unavailable)?;
    let Ok(Query(query)) = query else {
        return Err(invalid(
            "`after` is the number of the last event the client has",
        ));
    };
    let events = runs
        .events(&scope, &id, query.after)
        .map_err(|e| refused(&e))?;
    Ok(sse::stream(events, Some(state.shutdown.clone())).into_response())
}
