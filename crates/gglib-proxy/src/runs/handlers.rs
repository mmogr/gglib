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
use gglib_core::ports::{RunsError, RunsPort};
use serde::Deserialize;
use serde_json::Value;

use super::scope::Caller;
use super::sse;
use crate::models::ErrorResponse;
use crate::server::AppState;

type Answer = Result<Response, Response>;

fn error(status: StatusCode, error_type: &str, code: &str, message: String) -> Response {
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
fn invalid(message: &str) -> Response {
    error(
        StatusCode::BAD_REQUEST,
        "invalid_request_error",
        "invalid_request",
        message.to_owned(),
    )
}

/// The answer when this proxy was started without runs.
fn unavailable() -> Response {
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

/// `PUT /v1/runs/{id}`: start a chat run with the body as its request, or
/// answer with the caller's run that already has the id. 201 new, 200
/// existing.
pub(crate) async fn put_run(
    State(state): State<AppState>,
    Caller(scope): Caller,
    Path(id): Path<String>,
    body: Result<Json<Value>, JsonRejection>,
) -> Answer {
    let runs = runs(&state).ok_or_else(unavailable)?;
    let Ok(Json(body)) = body else {
        return Err(invalid(
            "a run's request body must be a JSON object, sent as application/json",
        ));
    };
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
