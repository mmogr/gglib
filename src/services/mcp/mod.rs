//! MCP (Model Context Protocol) server management.
//!
//! This module provides infrastructure for managing MCP servers that extend
//! gglib with external tools. Users can add MCP servers (stdio or SSE) which
//! automatically register their tools with the frontend Tool Registry.
//!
//! # Architecture
//!
//! ```text
//!                    McpService
//!                        │
//!        ┌───────────────┼───────────────┐
//!        │               │               │
//!   McpManager      McpClient       Database
//!   (lifecycle)    (JSON-RPC)      (configs)
//!        │               │
//!        └───────┬───────┘
//!                │
//!         MCP Server Process
//!         (stdio or SSE)
//! ```
//!
//! # Example
//!
//! ```rust,no_run
//! use gglib::services::mcp::{McpService, McpServerConfig, McpServerType};
//!
//! # async fn example(pool: &sqlx::SqlitePool) -> anyhow::Result<()> {
//! let mcp_service = McpService::new(pool.clone());
//!
//! // Add a new MCP server
//! let config = McpServerConfig {
//!     id: None,
//!     name: "Tavily Web Search".to_string(),
//!     server_type: McpServerType::Stdio,
//!     enabled: true,
//!     auto_start: false,
//!     command: Some("npx".to_string()),
//!     args: Some(vec!["-y".to_string(), "@tavily/mcp-server".to_string()]),
//!     cwd: None,
//!     url: None,
//!     env: vec![("TAVILY_API_KEY".to_string(), "tvly-xxx".to_string())],
//!     created_at: None,
//!     last_connected_at: None,
//! };
//!
//! let server = mcp_service.add_server(config).await?;
//! println!("Added server with id: {}", server.id.unwrap());
//!
//! // Start the server
//! mcp_service.start_server(&server.id.unwrap().to_string()).await?;
//!
//! // List available tools
//! let tools = mcp_service.list_server_tools(&server.id.unwrap().to_string()).await?;
//! for tool in tools {
//!     println!("Tool: {}", tool.name);
//! }
//! # Ok(())
//! # }
//! ```

mod client;
mod config;
mod database;
mod manager;
mod service;

pub use client::{McpClient, McpClientError};
pub use config::{
    McpServerConfig, McpServerInfo, McpServerStatus, McpServerType, McpTool, McpToolResult,
};
pub use database::{McpDatabase, McpDatabaseError};
pub use manager::{McpManager, McpManagerError};
pub use service::{McpService, McpServiceError};
