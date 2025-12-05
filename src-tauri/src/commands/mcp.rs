//! MCP (Model Context Protocol) server management commands.

use crate::app::AppState;
use gglib::services::mcp::{McpServerConfig, McpServerInfo, McpTool, McpToolResult};
use std::collections::HashMap;

/// Add a new MCP server configuration.
#[tauri::command]
pub async fn add_mcp_server(
    config: McpServerConfig,
    state: tauri::State<'_, AppState>,
) -> Result<McpServerConfig, String> {
    state
        .backend
        .core()
        .mcp()
        .add_server(config)
        .await
        .map_err(|e| format!("Failed to add MCP server: {}", e))
}

/// List all MCP server configurations with their current status.
#[tauri::command]
pub async fn list_mcp_servers(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<McpServerInfo>, String> {
    state
        .backend
        .core()
        .mcp()
        .list_servers_with_status()
        .await
        .map_err(|e| format!("Failed to list MCP servers: {}", e))
}

/// Update an MCP server configuration.
#[tauri::command]
pub async fn update_mcp_server(
    id: String,
    config: McpServerConfig,
    state: tauri::State<'_, AppState>,
) -> Result<McpServerConfig, String> {
    state
        .backend
        .core()
        .mcp()
        .update_server(&id, config)
        .await
        .map_err(|e| format!("Failed to update MCP server: {}", e))
}

/// Remove an MCP server configuration.
#[tauri::command]
pub async fn remove_mcp_server(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    state
        .backend
        .core()
        .mcp()
        .remove_server(&id)
        .await
        .map_err(|e| format!("Failed to remove MCP server: {}", e))
}

/// Start an MCP server.
#[tauri::command]
pub async fn start_mcp_server(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<McpTool>, String> {
    state
        .backend
        .core()
        .mcp()
        .start_server(&id)
        .await
        .map_err(|e| format!("Failed to start MCP server: {}", e))
}

/// Stop an MCP server.
#[tauri::command]
pub async fn stop_mcp_server(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    state
        .backend
        .core()
        .mcp()
        .stop_server(&id)
        .await
        .map_err(|e| format!("Failed to stop MCP server: {}", e))
}

/// Get all tools from all running MCP servers.
/// Returns a list of (server_id, tools) pairs.
#[tauri::command]
pub async fn list_mcp_tools(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<(String, Vec<McpTool>)>, String> {
    Ok(state.backend.core().mcp().list_all_tools().await)
}

/// Call an MCP tool.
#[tauri::command]
pub async fn call_mcp_tool(
    server_id: String,
    tool_name: String,
    arguments: serde_json::Value,
    state: tauri::State<'_, AppState>,
) -> Result<McpToolResult, String> {
    // Convert Value to HashMap
    let args_map: HashMap<String, serde_json::Value> = if let serde_json::Value::Object(map) =
        arguments
    {
        map.into_iter().collect()
    } else {
        HashMap::new()
    };

    state
        .backend
        .core()
        .mcp()
        .call_tool(&server_id, &tool_name, args_map)
        .await
        .map_err(|e| format!("Failed to call MCP tool: {}", e))
}
