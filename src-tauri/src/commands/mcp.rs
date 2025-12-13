//! MCP (Model Context Protocol) server management commands.
//!
//! These commands use the new clean architecture `McpService` from `gglib-mcp`.
//! All IDs are `i64` (no string parsing), and types come from `gglib-core` domain.

use crate::app::AppState;
use gglib_core::{McpServer, McpTool, McpToolResult, NewMcpServer, UpdateMcpServer};
use gglib_mcp::McpServerInfo;
use std::collections::HashMap;

/// Add a new MCP server configuration.
#[tauri::command]
pub async fn add_mcp_server(
    server: NewMcpServer,
    state: tauri::State<'_, AppState>,
) -> Result<McpServer, String> {
    state
        .mcp()
        .add_server(server)
        .await
        .map_err(|e| format!("Failed to add MCP server: {}", e))
}

/// List all MCP server configurations with their current status.
#[tauri::command]
pub async fn list_mcp_servers(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<McpServerInfo>, String> {
    state
        .mcp()
        .list_servers_with_status()
        .await
        .map_err(|e| format!("Failed to list MCP servers: {}", e))
}

/// Update an MCP server configuration.
///
/// Takes an ID and partial updates - only provided fields are changed.
#[tauri::command]
pub async fn update_mcp_server(
    id: i64,
    updates: UpdateMcpServer,
    state: tauri::State<'_, AppState>,
) -> Result<McpServer, String> {
    let mcp = state.mcp();
    
    // Get current server
    let servers = mcp
        .list_servers()
        .await
        .map_err(|e| format!("Failed to get servers: {}", e))?;
    
    let mut server = servers
        .into_iter()
        .find(|s| s.id == id)
        .ok_or_else(|| format!("Server with id {} not found", id))?;
    
    // Apply partial updates
    if let Some(name) = updates.name {
        server.name = name;
    }
    if let Some(server_type) = updates.server_type {
        server.server_type = server_type;
    }
    if let Some(config) = updates.config {
        server.config = config;
    }
    if let Some(enabled) = updates.enabled {
        server.enabled = enabled;
    }
    if let Some(auto_start) = updates.auto_start {
        server.auto_start = auto_start;
    }
    if let Some(env) = updates.env {
        server.env = env;
    }
    
    // Update in database
    mcp.update_server(server.clone())
        .await
        .map_err(|e| format!("Failed to update MCP server: {}", e))?;
    
    Ok(server)
}

/// Remove an MCP server configuration.
#[tauri::command]
pub async fn remove_mcp_server(
    id: i64,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    state
        .mcp()
        .remove_server(id)
        .await
        .map_err(|e| format!("Failed to remove MCP server: {}", e))
}

/// Start an MCP server.
#[tauri::command]
pub async fn start_mcp_server(
    id: i64,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<McpTool>, String> {
    state
        .mcp()
        .start_server(id)
        .await
        .map_err(|e| format!("Failed to start MCP server: {}", e))
}

/// Stop an MCP server.
#[tauri::command]
pub async fn stop_mcp_server(
    id: i64,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    state
        .mcp()
        .stop_server(id)
        .await
        .map_err(|e| format!("Failed to stop MCP server: {}", e))
}

/// Get all tools from all running MCP servers.
/// Returns a flattened list of all tools.
#[tauri::command]
pub async fn list_mcp_tools(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<McpTool>, String> {
    let tool_pairs = state.mcp().list_all_tools().await;
    Ok(tool_pairs.into_iter().flat_map(|(_, tools)| tools).collect())
}

/// Call an MCP tool.
#[tauri::command]
pub async fn call_mcp_tool(
    server_id: i64,
    tool_name: String,
    arguments: serde_json::Value,
    state: tauri::State<'_, AppState>,
) -> Result<McpToolResult, String> {
    // Convert Value to HashMap
    let args_map: HashMap<String, serde_json::Value> = match arguments {
        serde_json::Value::Object(map) => map.into_iter().collect(),
        _ => HashMap::new(),
    };

    state
        .mcp()
        .call_tool(server_id, &tool_name, args_map)
        .await
        .map_err(|e| format!("Failed to call MCP tool: {}", e))
}
