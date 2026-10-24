//! MCP handlers - Model Context Protocol server management.

use axum::Json;
use axum::extract::{Path, State};

use crate::error::HttpError;
use crate::state::AppState;
use gglib_app_services::types::{
    CreateMcpServerRequest, McpServerDto, McpServerInfo, McpTestResult, SetMcpRemoteRequest,
    UpdateMcpServerRequest,
};

/// List all MCP servers.
pub(crate) async fn list(
    State(state): State<AppState>,
) -> Result<Json<Vec<McpServerInfo>>, HttpError> {
    Ok(Json(state.mcp_ops.list().await?))
}

/// Add a new MCP server.
pub(crate) async fn add(
    State(state): State<AppState>,
    Json(req): Json<CreateMcpServerRequest>,
) -> Result<Json<McpServerInfo>, HttpError> {
    Ok(Json(state.mcp_ops.add(req).await?))
}

/// Update an MCP server.
pub(crate) async fn update(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<UpdateMcpServerRequest>,
) -> Result<Json<McpServerInfo>, HttpError> {
    Ok(Json(state.mcp_ops.update(id, req).await?))
}

/// Open an MCP server to paired devices, or close it: whether a device's
/// runs are offered its tools, from the next run on. Never stops it, and an
/// edit never changes it. 404 for an unknown id; 400 for opening an SSE
/// server or `builtin`.
pub(crate) async fn set_remote(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<SetMcpRemoteRequest>,
) -> Result<Json<McpServerDto>, HttpError> {
    Ok(Json(state.mcp_ops.set_allow_remote(id, req.allow).await?))
}

/// Remove an MCP server.
pub(crate) async fn remove(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<(), HttpError> {
    state.mcp_ops.remove(id).await?;
    Ok(())
}

/// Start an MCP server.
pub(crate) async fn start(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<McpServerInfo>, HttpError> {
    Ok(Json(state.mcp_ops.start(id).await?))
}

/// Stop an MCP server.
pub(crate) async fn stop(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<McpServerInfo>, HttpError> {
    Ok(Json(state.mcp_ops.stop(id).await?))
}

/// Test a server's stored configuration end to end — `gglib mcp test`.
///
/// Starts a throwaway instance, lists its tools, stops it. A failed
/// connection returns 200 with `ok: false` and the reason: a bad command is
/// the ordinary case this exists to diagnose, and the caller wants to render
/// it beside the config rather than handle an error.
pub(crate) async fn test_connection(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<McpTestResult>, HttpError> {
    Ok(Json(state.mcp_ops.test_connection(id).await?))
}

/// Resolve MCP server executable path (for diagnostics/auto-fix).
///
/// Returns 200 with success:false for resolution failures (not a 404/500).
pub(crate) async fn resolve_path(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<gglib_core::ports::ResolutionStatus>, HttpError> {
    Ok(Json(state.mcp_ops.resolve_path(id).await?))
}

#[cfg(test)]
#[path = "mcp_tests.rs"]
mod tests;
