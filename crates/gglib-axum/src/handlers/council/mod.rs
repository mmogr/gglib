//! Council endpoints: suggest a council and run a deliberation.

mod dto;

pub use dto::{CouncilRunRequest, CouncilSuggestRequest, CouncilSuggestResponse};

use std::convert::Infallible;

use axum::Json;
use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_core::Stream;
use futures_util::StreamExt as _;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::error::HttpError;
use crate::handlers::port_utils::validate_port;
use crate::state::AppState;

use dto::{COUNCIL_EVENT_CHANNEL_CAPACITY, CouncilEvent};

use gglib_agent::council::{run_council, suggest_council};
use gglib_core::domain::agent::AgentConfig;
use gglib_runtime::compose_council_ports;

// ─── POST /api/council/suggest ───────────────────────────────────────────────

pub async fn suggest(
    State(state): State<AppState>,
    Json(req): Json<CouncilSuggestRequest>,
) -> Result<Json<CouncilSuggestResponse>, HttpError> {
    validate_port(&state, req.port).await?;

    let ports = compose_council_ports(
        format!("http://127.0.0.1:{}", req.port),
        state.http_client.clone(),
        req.model.clone(),
        state.mcp.clone(),
    );

    let council = suggest_council(ports.llm, ports.tool_executor, &req.topic, req.agent_count)
        .await
        .map_err(|e| HttpError::Internal(e.to_string()))?;

    Ok(Json(CouncilSuggestResponse { council }))
}

// ─── POST /api/council/run ───────────────────────────────────────────────────

pub async fn run(
    State(state): State<AppState>,
    Json(req): Json<CouncilRunRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>> + Send + 'static>, HttpError> {
    let permit = state
        .agent_semaphore
        .clone()
        .try_acquire_owned()
        .map_err(|_| {
            HttpError::TooManyRequests("all agent loop slots are in use; try again later".into())
        })?;

    validate_port(&state, req.port).await?;

    let ports = compose_council_ports(
        format!("http://127.0.0.1:{}", req.port),
        state.http_client.clone(),
        req.model.clone(),
        state.mcp.clone(),
    );

    let agent_config: AgentConfig = req.config.map_or_else(AgentConfig::default, Into::into);
    let council_config = req.council;

    let (council_tx, council_rx) = mpsc::channel::<CouncilEvent>(COUNCIL_EVENT_CHANNEL_CAPACITY);

    tokio::spawn(async move {
        let _permit = permit;
        run_council(
            council_config,
            agent_config,
            ports.llm,
            ports.tool_executor,
            council_tx,
        )
        .await;
    });

    let sse_stream = ReceiverStream::new(council_rx).filter_map(|event| {
        futures_util::future::ready(match serde_json::to_string(&event) {
            Ok(json) => Some(Ok::<Event, Infallible>(Event::default().data(json))),
            Err(e) => {
                tracing::error!(error = %e, "council: failed to serialise CouncilEvent");
                let fallback = CouncilEvent::CouncilError {
                    message: "serialization failed".into(),
                };
                let json = serde_json::to_string(&fallback).unwrap_or_else(|_| {
                    r#"{"type":"council_error","message":"serialization failed"}"#.to_owned()
                });
                Some(Ok::<Event, Infallible>(Event::default().data(json)))
            }
        })
    });

    Ok(Sse::new(sse_stream).keep_alive(
        KeepAlive::new()
            .interval(std::time::Duration::from_secs(30))
            .text("ping"),
    ))
}
