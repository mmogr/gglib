//! `/v1/runs/*`: the same five calls as the daemon's `/api/runs/*`, in the
//! caller's scope.
//!
//! Every error message here is fixed text or a [`RunsError`]'s, which is
//! fixed text too: nothing echoes a request body or a frame.

use std::collections::BTreeSet;
use std::sync::Arc;

use axum::Json;
use axum::extract::rejection::{JsonRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
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
    /// `none`, or the ids of servers in the drawing answer's `mcp_servers`
    /// joined by commas (`3,7`): a chat run through the agent loop, for a
    /// chat the device keeps itself, offered the tools of those of the
    /// named servers that are open to paired devices (`gglib mcp
    /// allow-remote <server>`), or none of them; absent, a plain chat run,
    /// recorded as it always was.
    #[serde(default)]
    tools: Option<ToolsWord>,
    /// With `tools`: the message was sent with Draw pressed, so the image
    /// tool is offered, whichever servers `tools` names. Absent or `false`,
    /// it is not. Refused with `kind=agent`, whose turn says `draw` in its
    /// body.
    #[serde(default)]
    draw: bool,
    /// With `draw=true` beside `tools`: the image model the person chose for
    /// the picture, by its id in `GET /v1/images/drawing`'s `models`.
    /// Absent, the default rule draws. Refused with `kind=agent`, whose turn
    /// says `image_model` in its body, and on a plain chat run.
    #[serde(default)]
    image_model: Option<u32>,
}

/// What `?tools=` says (ADR 0026): `none`, or server ids joined by commas,
/// each in canonical decimal (`^[1-9][0-9]*$`), read into a set, so `7,3,7`
/// is `{3, 7}`. Any other value is refused: an empty one, an empty id
/// (`3,,7`), a sign, a leading zero, `0`, `none` beside an id, and any word,
/// `builtin` among them. A gglib that does not know a value so refuses it
/// rather than running tools the device keeps out. No value says every
/// open server: a device names each one it may use.
#[derive(Deserialize)]
#[serde(try_from = "String")]
struct ToolsWord(BTreeSet<i64>);

impl TryFrom<String> for ToolsWord {
    type Error = &'static str;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value == "none" {
            return Ok(Self(BTreeSet::new()));
        }
        value
            .split(',')
            .map(server_id)
            .collect::<Option<_>>()
            .map(Self)
            .ok_or("`tools` is `none` or the ids of servers joined by commas")
    }
}

/// One server id as `tools` spells it: digits alone, the first not `0`.
fn server_id(token: &str) -> Option<i64> {
    let canonical = token.bytes().all(|b| b.is_ascii_digit()) && !token.starts_with('0');
    canonical.then(|| token.parse().ok()).flatten()
}

/// `PUT /v1/runs/{id}`: start a chat run with the body as its request, or
/// answer with the caller's run that already has the id. 201 new, 200
/// existing. With `?kind=agent`, a device's turn on a hub chat, and with
/// `?tools=3,7` a chat run through the agent loop with the tools of those
/// of this machine's servers, or `?tools=none` one without its MCP tools:
/// see [`super::turn`].
pub(crate) async fn put_run(
    State(state): State<AppState>,
    Caller(scope): Caller,
    Path(id): Path<String>,
    query: Result<Query<PutQuery>, QueryRejection>,
    body: Result<Json<Value>, JsonRejection>,
) -> Answer {
    let Ok(Query(query)) = query else {
        return Err(invalid(
            "`kind` is `chat` or `agent`, `tools` is `none` or the ids of servers in \
             `mcp_servers` joined by commas, `draw` is `true` or `false`, and `image_model` is \
             an image model's id",
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
        return super::turn::put_chat(&state, scope, &id, body, drawing, tools.0).await;
    }
    if query.draw || query.image_model.is_some() {
        return Err(invalid(
            "`draw` and `image_model` go with `tools`: a plain chat run calls no tool",
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

#[cfg(test)]
#[path = "handlers_tests.rs"]
mod handlers_tests;
