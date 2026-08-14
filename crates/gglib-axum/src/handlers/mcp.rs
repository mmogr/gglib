//! MCP handlers - Model Context Protocol server management.

use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;

use crate::error::HttpError;
use crate::state::AppState;
use gglib_app_services::types::{
    CreateMcpServerRequest, McpServerInfo, McpTestResult, McpToolCallRequest, McpToolCallResponse,
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

/// Request body for calling an MCP tool (includes server ID).
#[derive(Debug, Deserialize)]
pub(crate) struct CallToolRequest {
    pub server_id: i64,
    #[serde(flatten)]
    pub call: McpToolCallRequest,
}

/// Call a tool on an MCP server.
pub(crate) async fn call_tool(
    State(state): State<AppState>,
    Json(req): Json<CallToolRequest>,
) -> Result<Json<McpToolCallResponse>, HttpError> {
    Ok(Json(
        state.mcp_ops.call_tool(req.server_id, req.call).await?,
    ))
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
