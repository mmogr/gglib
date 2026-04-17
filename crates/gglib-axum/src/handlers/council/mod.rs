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

use gglib_agent::AgentLoop;
use gglib_agent::council::prompts::COUNCIL_DESIGNER_PROMPT;
use gglib_agent::council::{SuggestedCouncil, run_council};
use gglib_core::AGENT_EVENT_CHANNEL_CAPACITY;
use gglib_core::domain::agent::{AgentConfig, AgentEvent, AgentMessage};
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

    #[allow(clippy::literal_string_with_formatting_args)]
    let system = COUNCIL_DESIGNER_PROMPT
        .replace("{agent_count}", &req.agent_count.to_string())
        .replace("{user_topic}", &req.topic);

    let messages = vec![
        AgentMessage::System { content: system },
        AgentMessage::User {
            content: req.topic.clone(),
        },
    ];

    let mut config = AgentConfig::default();
    config.max_iterations = 1;

    let agent = AgentLoop::build(ports.llm, ports.tool_executor, None);
    let (tx, mut rx) = mpsc::channel::<AgentEvent>(AGENT_EVENT_CHANNEL_CAPACITY);

    let handle = tokio::spawn(async move { agent.run(messages, config, tx).await });

    let mut content = String::new();
    while let Some(event) = rx.recv().await {
        if let AgentEvent::FinalAnswer { content: answer } = event {
            content = answer;
        }
    }
    let _ = handle.await;

    if content.is_empty() {
        return Err(HttpError::Internal(
            "LLM did not return a council suggestion".into(),
        ));
    }

    let mut council: SuggestedCouncil = parse_suggested_council(&content)?;
    council.backfill_defaults();
    Ok(Json(CouncilSuggestResponse { council }))
}

/// Extract the first JSON object from the LLM response (may be wrapped in markdown fences).
fn parse_suggested_council(raw: &str) -> Result<SuggestedCouncil, HttpError> {
    let trimmed = strip_markdown_json(raw);
    serde_json::from_str(trimmed)
        .map_err(|e| HttpError::Internal(format!("failed to parse council suggestion: {e}")))
}

/// Strip optional ` ```json ... ``` ` fences that small models often emit.
fn strip_markdown_json(s: &str) -> &str {
    let s = s.trim();
    let s = s.strip_prefix("```json").unwrap_or(s);
    let s = s.strip_prefix("```").unwrap_or(s);
    let s = s.strip_suffix("```").unwrap_or(s);
    s.trim()
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
