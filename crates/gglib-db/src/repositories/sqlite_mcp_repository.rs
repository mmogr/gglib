//! `SQLite` implementation of the MCP server repository.
//!
//! This module provides persistent storage for MCP server configurations
//! using `SQLite`. Environment variables are stored in a separate table with
//! base64 encoding (not encryption - a follow-up task should add proper
//! at-rest protection).

use async_trait::async_trait;
use base64::Engine;
use chrono::Utc;
use sqlx::SqlitePool;

use gglib_core::domain::mcp::{
    McpEnvEntry, McpLifecycle, McpServer, McpServerConfig, McpServerType, NewMcpServer,
};
use gglib_core::ports::{McpRepositoryError, McpServerRepository};

use super::row_mappers::parse_datetime;

/// Every `mcp_servers` column, as [`McpServerRow`] reads them.
const MCP_SERVER_COLUMNS: &str = "id, name, type, enabled, lifecycle, command, resolved_path_cache, args, cwd, path_extra, url, created_at, last_connected_at, is_valid, last_error";

/// Adds a server's row. It sets every column but the id and the two
/// timestamps, in the order [`SqliteMcpRepository::write`] binds them.
const INSERT_SERVER: &str = "INSERT INTO mcp_servers \
     (name, type, enabled, lifecycle, command, resolved_path_cache, args, cwd, path_extra, url, is_valid, last_error) \
     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)";

/// Replaces a server's row: the columns [`INSERT_SERVER`] sets, in its
/// order, then the id of the row.
const UPDATE_SERVER: &str = "UPDATE mcp_servers \
     SET name = ?, type = ?, enabled = ?, lifecycle = ?, command = ?, resolved_path_cache = ?, args = ?, cwd = ?, path_extra = ?, url = ?, is_valid = ?, last_error = ? \
     WHERE id = ?";

/// `SQLite` implementation of the MCP server repository.
pub struct SqliteMcpRepository {
    pool: SqlitePool,
}

impl SqliteMcpRepository {
    /// Create a new `SQLite` MCP repository.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal row types for database queries
// ─────────────────────────────────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct McpServerRow {
    id: i64,
    name: String,
    #[sqlx(rename = "type")]
    server_type: String,
    enabled: bool,
    lifecycle: String,
    command: Option<String>,
    resolved_path_cache: Option<String>,
    args: Option<String>,
    cwd: Option<String>,
    path_extra: Option<String>,
    url: Option<String>,
    created_at: String,
    last_connected_at: Option<String>,
    is_valid: bool,
    last_error: Option<String>,
}

#[derive(sqlx::FromRow)]
struct EnvRow {
    key: String,
    value: String,
}

/// What a write sets, borrowed from the server being added or replaced.
struct Written<'a> {
    name: &'a str,
    server_type: McpServerType,
    config: &'a McpServerConfig,
    enabled: bool,
    lifecycle: McpLifecycle,
    env: &'a [McpEnvEntry],
    is_valid: bool,
    last_error: Option<&'a str>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Helper functions
// ─────────────────────────────────────────────────────────────────────────────

/// Convert a `McpServerRow` (with env) to domain `McpServer`.
///
/// The schema's CHECK constraints admit only the strings the type and the
/// lifecycle parse, so the defaults here are never taken.
fn row_to_server(row: McpServerRow, env: Vec<McpEnvEntry>) -> McpServer {
    let config = McpServerConfig {
        command: row.command,
        resolved_path_cache: row.resolved_path_cache,
        args: row.args.and_then(|a| serde_json::from_str(&a).ok()),
        working_dir: row.cwd,
        path_extra: row.path_extra,
        url: row.url,
    };

    McpServer {
        id: row.id,
        name: row.name,
        server_type: row.server_type.parse::<McpServerType>().unwrap_or_default(),
        config,
        enabled: row.enabled,
        lifecycle: row.lifecycle.parse::<McpLifecycle>().unwrap_or_default(),
        env,
        created_at: parse_datetime(Some(row.created_at)).unwrap_or_else(Utc::now),
        last_connected_at: parse_datetime(row.last_connected_at),
        is_valid: row.is_valid,
        last_error: row.last_error,
    }
}

/// Decode a base64-encoded environment variable value.
fn decode_env_value(encoded: &str) -> Result<String, McpRepositoryError> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|e| McpRepositoryError::Internal(format!("Failed to decode env var: {e}")))?;

    String::from_utf8(bytes)
        .map_err(|e| McpRepositoryError::Internal(format!("Invalid UTF-8 in env var: {e}")))
}

/// Encode an environment variable value to base64.
fn encode_env_value(value: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(value.as_bytes())
}

/// A failure of the store itself, as the port reports it.
fn internal(e: impl std::fmt::Display) -> McpRepositoryError {
    McpRepositoryError::Internal(e.to_string())
}

// ─────────────────────────────────────────────────────────────────────────────
// Repository implementation
// ─────────────────────────────────────────────────────────────────────────────

#[async_trait]
impl McpServerRepository for SqliteMcpRepository {
    async fn insert(&self, server: NewMcpServer) -> Result<McpServer, McpRepositoryError> {
        let id = self
            .write(
                None,
                Written {
                    name: &server.name,
                    server_type: server.server_type,
                    config: &server.config,
                    enabled: server.enabled,
                    lifecycle: server.lifecycle,
                    env: &server.env,
                    // Not valid until the service has validated it.
                    is_valid: false,
                    last_error: None,
                },
            )
            .await?;

        self.get_by_id(id).await
    }

    async fn get_by_id(&self, id: i64) -> Result<McpServer, McpRepositoryError> {
        let row = sqlx::query_as::<_, McpServerRow>(&format!(
            "SELECT {MCP_SERVER_COLUMNS} FROM mcp_servers WHERE id = ?"
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(internal)?
        .ok_or_else(|| McpRepositoryError::NotFound(id.to_string()))?;

        let env = self.fetch_env(id).await?;

        Ok(row_to_server(row, env))
    }

    async fn get_by_name(&self, name: &str) -> Result<McpServer, McpRepositoryError> {
        let row = sqlx::query_as::<_, McpServerRow>(&format!(
            "SELECT {MCP_SERVER_COLUMNS} FROM mcp_servers WHERE name = ?"
        ))
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(internal)?
        .ok_or_else(|| McpRepositoryError::NotFound(name.to_string()))?;

        let env = self.fetch_env(row.id).await?;

        Ok(row_to_server(row, env))
    }

    async fn list(&self) -> Result<Vec<McpServer>, McpRepositoryError> {
        let rows = sqlx::query_as::<_, McpServerRow>(&format!(
            "SELECT {MCP_SERVER_COLUMNS} FROM mcp_servers ORDER BY name"
        ))
        .fetch_all(&self.pool)
        .await
        .map_err(internal)?;

        let mut servers = Vec::with_capacity(rows.len());
        for row in rows {
            let env = self.fetch_env(row.id).await?;
            servers.push(row_to_server(row, env));
        }

        Ok(servers)
    }

    async fn update(&self, server: &McpServer) -> Result<(), McpRepositoryError> {
        self.write(
            Some(server.id),
            Written {
                name: &server.name,
                server_type: server.server_type,
                config: &server.config,
                enabled: server.enabled,
                lifecycle: server.lifecycle,
                env: &server.env,
                is_valid: server.is_valid,
                last_error: server.last_error.as_deref(),
            },
        )
        .await?;

        Ok(())
    }

    async fn delete(&self, id: i64) -> Result<(), McpRepositoryError> {
        // Verify server exists
        let _ = self.get_by_id(id).await?;

        // Env vars are deleted via ON DELETE CASCADE
        sqlx::query("DELETE FROM mcp_servers WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(internal)?;

        Ok(())
    }

    async fn update_last_connected(&self, id: i64) -> Result<(), McpRepositoryError> {
        let result =
            sqlx::query("UPDATE mcp_servers SET last_connected_at = datetime('now') WHERE id = ?")
                .bind(id)
                .execute(&self.pool)
                .await
                .map_err(internal)?;

        if result.rows_affected() == 0 {
            return Err(McpRepositoryError::NotFound(id.to_string()));
        }

        Ok(())
    }
}

impl SqliteMcpRepository {
    /// Write a server whole, in one transaction: its row, then its env rows
    /// in place of any the row had. `id` names the row to replace; without
    /// one a row is added. Returns the row's id.
    ///
    /// A failure at any step rolls the transaction back, so the database
    /// never holds a server without its env, nor half of an update.
    async fn write(&self, id: Option<i64>, server: Written<'_>) -> Result<i64, McpRepositoryError> {
        // `args` is NOT NULL: a server with none, as every SSE server is,
        // stores an empty list.
        let args = serde_json::to_string(server.config.args.as_deref().unwrap_or_default())
            .map_err(internal)?;

        let mut tx = self.pool.begin().await.map_err(internal)?;

        let mut query = sqlx::query(if id.is_some() {
            UPDATE_SERVER
        } else {
            INSERT_SERVER
        })
        .bind(server.name)
        .bind(server.server_type.to_string())
        .bind(server.enabled)
        .bind(server.lifecycle.to_string())
        .bind(&server.config.command)
        .bind(&server.config.resolved_path_cache)
        .bind(args)
        .bind(&server.config.working_dir)
        .bind(&server.config.path_extra)
        .bind(&server.config.url)
        .bind(server.is_valid)
        .bind(server.last_error);
        if let Some(id) = id {
            query = query.bind(id);
        }
        let written = query.execute(&mut *tx).await.map_err(internal)?;

        let id = match id {
            Some(id) if written.rows_affected() == 0 => {
                return Err(McpRepositoryError::NotFound(id.to_string()));
            }
            Some(id) => {
                sqlx::query("DELETE FROM mcp_server_env WHERE server_id = ?")
                    .bind(id)
                    .execute(&mut *tx)
                    .await
                    .map_err(internal)?;
                id
            }
            None => written.last_insert_rowid(),
        };

        for entry in server.env {
            sqlx::query("INSERT INTO mcp_server_env (server_id, key, value) VALUES (?, ?, ?)")
                .bind(id)
                .bind(&entry.key)
                .bind(encode_env_value(&entry.value))
                .execute(&mut *tx)
                .await
                .map_err(internal)?;
        }

        tx.commit().await.map_err(internal)?;

        Ok(id)
    }

    /// Fetch and decode environment variables for a server.
    async fn fetch_env(&self, server_id: i64) -> Result<Vec<McpEnvEntry>, McpRepositoryError> {
        let rows = sqlx::query_as::<_, EnvRow>(
            "SELECT key, value FROM mcp_server_env WHERE server_id = ?",
        )
        .bind(server_id)
        .fetch_all(&self.pool)
        .await
        .map_err(internal)?;

        let mut env = Vec::with_capacity(rows.len());
        for row in rows {
            let decoded_value = decode_env_value(&row.value)?;
            env.push(McpEnvEntry::new(row.key, decoded_value));
        }

        Ok(env)
    }
}

#[cfg(test)]
#[path = "sqlite_mcp_repository_tests.rs"]
mod tests;
