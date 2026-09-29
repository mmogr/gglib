#![doc = include_str!("README.md")]

use axum::Json;
use axum::extract::rejection::{JsonRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, Sse};
use futures_util::stream::Stream;
use gglib_core::domain::runs::{RunInfo, RunList};
use gglib_core::ports::{RunScope, RunsPort};
use serde::Deserialize;
use serde_json::Value;
use std::convert::Infallible;

use crate::error::HttpError;
use crate::state::AppState;

/// This door is the daemon's own, so every call is this machine's.
const SCOPE: RunScope = RunScope::Local;

/// A request the route could not read, answered as `invalid_request`.
fn invalid(message: String) -> HttpError {
    HttpError::Coded {
        status: StatusCode::BAD_REQUEST,
        code: "invalid_request",
        message,
    }
}

/// `PUT /api/runs/{id}`: start a chat run with the body as its request, or
/// answer with the run that already has the id. 201 new, 200 existing.
pub(crate) async fn put(
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Result<Json<Value>, JsonRejection>,
) -> Result<(StatusCode, Json<RunInfo>), HttpError> {
    let Json(body) = body.map_err(|e| invalid(e.body_text()))?;
    let created = state.runs.create(SCOPE, &id, body)?;
    let status = if created.created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, Json(created.info)))
}

/// `GET /api/runs`: every run, newest first.
pub(crate) async fn list(State(state): State<AppState>) -> Json<RunList> {
    Json(state.runs.list(&SCOPE))
}

/// `GET /api/runs/{id}`.
pub(crate) async fn get(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<RunInfo>, HttpError> {
    Ok(Json(state.runs.get(&SCOPE, &id)?))
}

/// `POST /api/runs/{id}/cancel`: idempotent.
pub(crate) async fn cancel(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<RunInfo>, HttpError> {
    Ok(Json(state.runs.cancel(&SCOPE, &id)?))
}

#[derive(Deserialize)]
pub(crate) struct EventsQuery {
    /// The last event the client has; 0 for all of them.
    #[serde(default)]
    after: u32,
}

/// `GET /api/runs/{id}/events?after=N`: see [`gglib_proxy::runs::sse`].
pub(crate) async fn events(
    State(state): State<AppState>,
    Path(id): Path<String>,
    query: Result<Query<EventsQuery>, QueryRejection>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>> + Send + 'static>, HttpError> {
    let Query(query) = query.map_err(|e| invalid(e.body_text()))?;
    let events = state.runs.events(&SCOPE, &id, query.after)?;
    Ok(gglib_proxy::runs::sse::stream(
        events,
        state.daemon_shutdown.clone(),
    ))
}
