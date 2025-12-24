//! Server handlers - model server start/stop operations.

use axum::Json;
use axum::extract::{Path, State};

use crate::error::HttpError;
use crate::state::AppState;
use gglib_gui::types::{ServerInfo, StartServerRequest, StartServerResponse};

/// List all running servers.
pub async fn list(State(state): State<AppState>) -> Json<Vec<ServerInfo>> {
    Json(state.gui.list_servers().await)
}

// ============================================================================
// Path-based handlers (legacy: /api/servers/{id}/start, /api/servers/{id}/stop)
// ============================================================================

/// Start a model server (path-based: model ID in URL).
pub async fn start(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<StartServerRequest>,
) -> Result<Json<StartServerResponse>, HttpError> {
    Ok(Json(state.gui.start_server(id, req).await?))
}

/// Stop a model server (path-based: model ID in URL).
pub async fn stop(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<String>, HttpError> {
    Ok(Json(state.gui.stop_server(id).await?))
}

// ============================================================================
// Body-based handlers (collection routes: /api/servers/start, /api/servers/stop)
// Frontend sends { id, ...config } for start, { model_id } for stop.
// ============================================================================

/// Request body for starting a server via collection route.
/// Maps frontend's `id` field to backend's `model_id`.
#[derive(Debug, serde::Deserialize)]
pub struct StartServerBody {
    /// Model ID - frontend sends as `id`
    #[serde(alias = "id")]
    pub model_id: Option<i64>,
    #[serde(flatten)]
    pub config: StartServerRequest,
}

/// Start a model server (body-based: model ID in request body).
pub async fn start_body(
    State(state): State<AppState>,
    Json(body): Json<StartServerBody>,
) -> Result<Json<StartServerResponse>, HttpError> {
    let model_id = body.model_id.ok_or_else(|| {
        HttpError::BadRequest("Missing model_id (or id) in request body".to_string())
    })?;
    Ok(Json(state.gui.start_server(model_id, body.config).await?))
}

/// Request body for stopping a server via collection route.
#[derive(Debug, serde::Deserialize)]
pub struct StopServerBody {
    pub model_id: i64,
}

/// Stop a model server (body-based: model ID in request body).
pub async fn stop_body(
    State(state): State<AppState>,
    Json(body): Json<StopServerBody>,
) -> Result<Json<String>, HttpError> {
    Ok(Json(state.gui.stop_server(body.model_id).await?))
}

// ============================================================================
// Server log handlers
// ============================================================================

use axum::response::sse::{Event, Sse};
use futures_util::stream::Stream;
use std::convert::Infallible;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

/// Get initial server logs for a specific port (REST endpoint).
pub async fn get_logs(
    State(state): State<AppState>,
    Path(port): Path<u16>,
) -> Json<Vec<gglib_gui::types::ServerLogEntry>> {
    Json(state.gui.get_server_logs(port))
}

/// Stream server logs via SSE for a specific port.
///
/// Subscribes to the global log broadcast and filters by port.
/// Includes keep-alive pings every 30 seconds to prevent proxy timeouts.
pub async fn stream_logs(
    State(state): State<AppState>,
    Path(port): Path<u16>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>> + Send + 'static> {
    let receiver = state.gui.subscribe_server_logs();
    
    let stream = BroadcastStream::new(receiver).filter_map(move |result| {
        match result {
            Ok(entry) => {
                // Only emit logs for the requested port
                if entry.port != port {
                    return None;
                }
                
                // Serialize log entry to JSON
                match serde_json::to_string(&entry) {
                    Ok(json) => Some(Ok(Event::default().data(json))),
                    Err(e) => {
                        tracing::warn!("Failed to serialize log entry: {}", e);
                        None
                    }
                }
            }
            Err(e) => {
                tracing::debug!("Log stream error: {}", e);
                None
            }
        }
    });

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(30))
            .text("ping"),
    )
}

/// Clear logs for a specific server port (DELETE endpoint).
pub async fn clear_logs(
    State(state): State<AppState>,
    Path(port): Path<u16>,
) -> Json<String> {
    state.gui.clear_server_logs(port);
    Json(format!("Logs cleared for port {}", port))
}
