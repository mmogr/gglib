//! MCP (Model Context Protocol) server management.
//!
//! This crate provides protocol client, lifecycle management, and service facade
//! for MCP servers. It depends only on `gglib-core` for domain types and trait ports,
//! keeping storage and process implementation details in their respective crates.
//!
//! # Architecture
//!
//! - **Domain types**: `McpServer`, `NewMcpServer`, etc. (from `gglib-core::domain::mcp`)
//! - **Repository trait**: `McpServerRepository` (from `gglib-core::ports`)
//! - **Process abstraction**: `ProcessRunner` (from `gglib-core::ports`)
//! - **Event emission**: `AppEventEmitter` (from `gglib-core::ports`)
//!
//! # Components
//!
//! - `client` - JSON-RPC 2.0 MCP protocol client
//! - `manager` - Server lifecycle management (start/stop/status)
//! - `service` - High-level service facade with DI
//!
//! # Example
//!
//! ```ignore
//! use gglib_mcp::McpService;
//!
//! // Create service with injected dependencies
//! let service = McpService::new(repo, runner, emitter);
//!
//! // Add and start a server
//! let server = service.add_server(NewMcpServer::new_stdio("my-mcp", "npx", vec![])).await?;
//! let tools = service.start_server(server.id).await?;
//! ```

#![deny(unsafe_code)]
#![deny(unused_crate_dependencies)]

pub mod client;
pub mod manager;
pub mod service;

// Re-export domain types from core for convenience
pub use gglib_core::{
    McpEnvEntry, McpServer, McpServerConfig, McpServerStatus, McpServerType, McpTool,
    McpToolResult, NewMcpServer,
};

// Re-export this crate's public types
pub use client::{InitializeResult, McpClient, McpClientError, ServerCapabilities, ServerInfo};
pub use manager::{McpManager, McpManagerError};
pub use service::McpService;
