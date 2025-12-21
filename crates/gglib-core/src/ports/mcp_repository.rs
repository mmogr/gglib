//! MCP server repository trait and error types.
//!
//! This module defines the repository abstraction for MCP server persistence.

use async_trait::async_trait;
use thiserror::Error;

use crate::domain::mcp::{McpServer, NewMcpServer};

/// Domain-specific errors for MCP repository operations.
///
/// This error type abstracts away storage implementation details and provides
/// a clean interface for services to handle MCP storage failures.
#[derive(Debug, Error)]
pub enum McpRepositoryError {
    /// The requested MCP server was not found.
    #[error("MCP server not found: {0}")]
    NotFound(String),

    /// An MCP server with the same name already exists.
    #[error("MCP server already exists: {0}")]
    Conflict(String),

    /// Storage backend error (database, etc.).
    #[error("Storage error: {0}")]
    Internal(String),
}

/// Repository trait for MCP server persistence.
///
/// This trait defines the interface for storing and retrieving MCP server
/// configurations. Implementations handle all persistence details internally.
///
/// # Design Rules
///
/// - Environment variables are embedded in `McpServer` - no separate env API
/// - `update()` replaces the entire server including env atomically
/// - Constraint: unique `name` across all servers
///
/// # Example
///
/// ```ignore
/// // Insert a new server
/// let server = repo.insert(NewMcpServer::new_stdio("my-server", "npx", vec![])).await?;
///
/// // Get by ID
/// let found = repo.get_by_id(server.id).await?;
///
/// // List all servers
/// let all = repo.list().await?;
/// ```
#[async_trait]
pub trait McpServerRepository: Send + Sync {
    /// Insert a new MCP server.
    ///
    /// Returns the server with its assigned ID and timestamps.
    ///
    /// # Errors
    ///
    /// - `Conflict` if a server with the same name already exists
    /// - `Internal` for storage errors
    async fn insert(&self, server: NewMcpServer) -> Result<McpServer, McpRepositoryError>;

    /// Get an MCP server by its database ID.
    ///
    /// # Errors
    ///
    /// - `NotFound` if no server with the given ID exists
    /// - `Internal` for storage errors
    async fn get_by_id(&self, id: i64) -> Result<McpServer, McpRepositoryError>;

    /// Get an MCP server by its unique name.
    ///
    /// # Errors
    ///
    /// - `NotFound` if no server with the given name exists
    /// - `Internal` for storage errors
    async fn get_by_name(&self, name: &str) -> Result<McpServer, McpRepositoryError>;

    /// List all MCP servers.
    ///
    /// # Errors
    ///
    /// - `Internal` for storage errors
    async fn list(&self) -> Result<Vec<McpServer>, McpRepositoryError>;

    /// Update an existing MCP server.
    ///
    /// This atomically replaces the entire server including environment variables.
    ///
    /// # Errors
    ///
    /// - `NotFound` if no server with the given ID exists
    /// - `Conflict` if the new name conflicts with another server
    /// - `Internal` for storage errors
    async fn update(&self, server: &McpServer) -> Result<(), McpRepositoryError>;

    /// Delete an MCP server by its database ID.
    ///
    /// # Errors
    ///
    /// - `NotFound` if no server with the given ID exists
    /// - `Internal` for storage errors
    async fn delete(&self, id: i64) -> Result<(), McpRepositoryError>;

    /// Update only the `last_connected_at` timestamp.
    ///
    /// This is a narrow, first-class method for updating connection time
    /// without replacing the entire server.
    ///
    /// # Errors
    ///
    /// - `NotFound` if no server with the given ID exists
    /// - `Internal` for storage errors
    async fn update_last_connected(&self, id: i64) -> Result<(), McpRepositoryError>;
}
