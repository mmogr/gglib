//! MCP handlers - Model Context Protocol server management.

use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;

use crate::error::HttpError;
use crate::state::AppState;
use gglib_app_services::types::{
    CreateMcpServerRequest, McpServerInfo, McpToolCallRequest, McpToolCallResponse, McpToolInfo,
    UpdateMcpServerRequest,
};

/// List all MCP servers.
pub async fn list(State(state): State<AppState>) -> Result<Json<Vec<McpServerInfo>>, HttpError> {
    Ok(Json(state.mcp_ops.list().await?))
}

/// Add a new MCP server.
pub async fn add(
    State(state): State<AppState>,
    Json(req): Json<CreateMcpServerRequest>,
) -> Result<Json<McpServerInfo>, HttpError> {
    Ok(Json(state.mcp_ops.add(req).await?))
}

/// Update an MCP server.
pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<UpdateMcpServerRequest>,
) -> Result<Json<McpServerInfo>, HttpError> {
    Ok(Json(state.mcp_ops.update(id, req).await?))
}

/// Remove an MCP server.
pub async fn remove(State(state): State<AppState>, Path(id): Path<i64>) -> Result<(), HttpError> {
    state.mcp_ops.remove(id).await?;
    Ok(())
}

/// Start an MCP server.
pub async fn start(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<McpServerInfo>, HttpError> {
    Ok(Json(state.mcp_ops.start(id).await?))
}

/// Stop an MCP server.
pub async fn stop(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<McpServerInfo>, HttpError> {
    Ok(Json(state.mcp_ops.stop(id).await?))
}

/// List tools from a running MCP server.
pub async fn list_tools(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Vec<McpToolInfo>>, HttpError> {
    Ok(Json(state.mcp_ops.list_tools(id).await?))
}

/// Request body for calling an MCP tool (includes server ID).
#[derive(Debug, Deserialize)]
pub struct CallToolRequest {
    pub server_id: i64,
    #[serde(flatten)]
    pub call: McpToolCallRequest,
}

/// Call a tool on an MCP server.
pub async fn call_tool(
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
pub async fn resolve_path(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<gglib_core::ports::ResolutionStatus>, HttpError> {
    Ok(Json(state.mcp_ops.resolve_path(id).await?))
}
