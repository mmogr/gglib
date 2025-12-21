//! MCP handlers - Model Context Protocol server management.

use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;

use crate::error::HttpError;
use crate::state::AppState;
use gglib_gui::types::{
    CreateMcpServerRequest, McpServerInfo, McpToolCallRequest, McpToolCallResponse, McpToolInfo,
    UpdateMcpServerRequest,
};

/// List all MCP servers.
pub async fn list(State(state): State<AppState>) -> Result<Json<Vec<McpServerInfo>>, HttpError> {
    Ok(Json(state.gui.list_mcp_servers().await?))
}

/// Add a new MCP server.
pub async fn add(
    State(state): State<AppState>,
    Json(req): Json<CreateMcpServerRequest>,
) -> Result<Json<McpServerInfo>, HttpError> {
    Ok(Json(state.gui.add_mcp_server(req).await?))
}

/// Update an MCP server.
pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<UpdateMcpServerRequest>,
) -> Result<Json<McpServerInfo>, HttpError> {
    Ok(Json(state.gui.update_mcp_server(id, req).await?))
}

/// Remove an MCP server.
pub async fn remove(State(state): State<AppState>, Path(id): Path<i64>) -> Result<(), HttpError> {
    state.gui.remove_mcp_server(id).await?;
    Ok(())
}

/// Start an MCP server.
pub async fn start(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<McpServerInfo>, HttpError> {
    Ok(Json(state.gui.start_mcp_server(id).await?))
}

/// Stop an MCP server.
pub async fn stop(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<McpServerInfo>, HttpError> {
    Ok(Json(state.gui.stop_mcp_server(id).await?))
}

/// List tools from a running MCP server.
pub async fn list_tools(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Vec<McpToolInfo>>, HttpError> {
    Ok(Json(state.gui.list_mcp_tools(id).await?))
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
        state.gui.call_mcp_tool(req.server_id, req.call).await?,
    ))
}

/// Resolve MCP server executable path (for diagnostics/auto-fix).
///
/// Returns 200 with success:false for resolution failures (not a 404/500).
pub async fn resolve_path(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<gglib_core::ports::ResolutionStatus>, HttpError> {
    Ok(Json(state.gui.resolve_mcp_server_path(id).await?))
}
