//! `SQLite` implementation of the MCP server repository.
//!
//! This module provides persistent storage for MCP server configurations
//! using `SQLite`. Environment variables are stored in a separate table with
//! base64 encoding (not encryption - a follow-up task should add proper
//! at-rest protection).

use async_trait::async_trait;
use base64::Engine;
use chrono::{DateTime, TimeZone, Utc};
use sqlx::SqlitePool;

use gglib_core::domain::mcp::{
    McpEnvEntry, McpServer, McpServerConfig, McpServerType, NewMcpServer,
};
use gglib_core::ports::{McpRepositoryError, McpServerRepository};

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
    auto_start: bool,
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

// ─────────────────────────────────────────────────────────────────────────────
// Helper functions
// ─────────────────────────────────────────────────────────────────────────────

/// Parse a datetime string from `SQLite` to a `DateTime<Utc>`.
fn parse_datetime(s: &str) -> DateTime<Utc> {
    // `SQLite` stores datetime as "YYYY-MM-DD HH:MM:SS" format
    chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
        .map(|dt| Utc.from_utc_datetime(&dt))
        .unwrap_or_else(|_| Utc::now())
}

/// Convert a `McpServerRow` (with env) to domain `McpServer`.
fn row_to_server(row: McpServerRow, env: Vec<McpEnvEntry>) -> McpServer {
    let server_type = match row.server_type.as_str() {
        "sse" => McpServerType::Sse,
        _ => McpServerType::Stdio,
    };

    let args: Option<Vec<String>> = row.args.and_then(|a| serde_json::from_str(&a).ok());

    let config = McpServerConfig {
        command: row.command,
        resolved_path_cache: row.resolved_path_cache,
        args,
        working_dir: row.cwd,
        path_extra: row.path_extra,
        url: row.url,
    };

    McpServer {
        id: row.id,
        name: row.name,
        server_type,
        config,
        enabled: row.enabled,
        auto_start: row.auto_start,
        env,
        created_at: parse_datetime(&row.created_at),
        last_connected_at: row.last_connected_at.as_ref().map(|s| parse_datetime(s)),
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

/// Map `SQLx` errors to `McpRepositoryError`.
fn map_sqlx_error(e: sqlx::Error) -> McpRepositoryError {
    // Check for unique constraint violations (name conflict)
    let msg = e.to_string();
    if msg.contains("UNIQUE constraint failed") && msg.contains("name") {
        return McpRepositoryError::Conflict("MCP server name already exists".to_string());
    }
    McpRepositoryError::Internal(e.to_string())
}

// ─────────────────────────────────────────────────────────────────────────────
// Repository implementation
// ─────────────────────────────────────────────────────────────────────────────

#[async_trait]
impl McpServerRepository for SqliteMcpRepository {
    async fn insert(&self, server: NewMcpServer) -> Result<McpServer, McpRepositoryError> {
        let server_type = match server.server_type {
            McpServerType::Stdio => "stdio",
            McpServerType::Sse => "sse",
        };

        let args_json = server
            .config
            .args
            .as_ref()
            .map(|a| serde_json::to_string(a).unwrap_or_else(|_| "[]".to_string()));

        // Insert the server
        let result = sqlx::query(
            r#"
            INSERT INTO mcp_servers (name, type, enabled, auto_start, command, resolved_path_cache, args, cwd, path_extra, url, is_valid, last_error)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&server.name)
        .bind(server_type)
        .bind(server.enabled)
        .bind(server.auto_start)
        .bind(&server.config.command)
        .bind(&server.config.resolved_path_cache)
        .bind(&args_json)
        .bind(&server.config.working_dir)
        .bind(&server.config.path_extra)
        .bind(&server.config.url)
        .bind(0) // is_valid starts as 0, will be validated on startup
        .bind(Option::<String>::None) // last_error starts as None
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let server_id = result.last_insert_rowid();

        // Insert environment variables
        for entry in &server.env {
            let encoded_value = encode_env_value(&entry.value);

            sqlx::query("INSERT INTO mcp_server_env (server_id, key, value) VALUES (?, ?, ?)")
                .bind(server_id)
                .bind(&entry.key)
                .bind(&encoded_value)
                .execute(&self.pool)
                .await
                .map_err(map_sqlx_error)?;
        }

        // Fetch and return the complete server
        self.get_by_id(server_id).await
    }

    async fn get_by_id(&self, id: i64) -> Result<McpServer, McpRepositoryError> {
        let row = sqlx::query_as::<_, McpServerRow>(
            r#"
            SELECT id, name, type, enabled, auto_start, command, resolved_path_cache, args, cwd, path_extra, url, 
                   created_at, last_connected_at, is_valid, last_error
            FROM mcp_servers WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?
        .ok_or_else(|| McpRepositoryError::NotFound(id.to_string()))?;

        // Fetch environment variables
        let env = self.fetch_env(id).await?;

        Ok(row_to_server(row, env))
    }

    async fn get_by_name(&self, name: &str) -> Result<McpServer, McpRepositoryError> {
        let row = sqlx::query_as::<_, McpServerRow>(
            r#"
            SELECT id, name, type, enabled, auto_start, command, resolved_path_cache, args, cwd, path_extra, url, 
                   created_at, last_connected_at, is_valid, last_error
            FROM mcp_servers WHERE name = ?
            "#,
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?
        .ok_or_else(|| McpRepositoryError::NotFound(name.to_string()))?;

        // Fetch environment variables
        let env = self.fetch_env(row.id).await?;

        Ok(row_to_server(row, env))
    }

    async fn list(&self) -> Result<Vec<McpServer>, McpRepositoryError> {
        let rows = sqlx::query_as::<_, McpServerRow>(
            r#"
            SELECT id, name, type, enabled, auto_start, command, resolved_path_cache, args, cwd, path_extra, url, 
                   created_at, last_connected_at, is_valid, last_error
            FROM mcp_servers ORDER BY name
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let mut servers = Vec::with_capacity(rows.len());
        for row in rows {
            let env = self.fetch_env(row.id).await?;
            servers.push(row_to_server(row, env));
        }

        Ok(servers)
    }

    async fn update(&self, server: &McpServer) -> Result<(), McpRepositoryError> {
        // Verify server exists
        let _ = self.get_by_id(server.id).await?;

        let server_type = match server.server_type {
            McpServerType::Stdio => "stdio",
            McpServerType::Sse => "sse",
        };

        let args_json = server
            .config
            .args
            .as_ref()
            .map(|a| serde_json::to_string(a).unwrap_or_else(|_| "[]".to_string()));

        // Update the server
        sqlx::query(
            r#"
            UPDATE mcp_servers 
            SET name = ?, type = ?, enabled = ?, auto_start = ?, command = ?, resolved_path_cache = ?, args = ?, cwd = ?, path_extra = ?, url = ?, is_valid = ?, last_error = ?
            WHERE id = ?
            "#,
        )
        .bind(&server.name)
        .bind(server_type)
        .bind(server.enabled)
        .bind(server.auto_start)
        .bind(&server.config.command)
        .bind(&server.config.resolved_path_cache)
        .bind(&args_json)
        .bind(&server.config.working_dir)
        .bind(&server.config.path_extra)
        .bind(&server.config.url)
        .bind(server.is_valid)
        .bind(&server.last_error)
        .bind(server.id)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        // Atomic env replacement: delete all and re-insert
        sqlx::query("DELETE FROM mcp_server_env WHERE server_id = ?")
            .bind(server.id)
            .execute(&self.pool)
            .await
            .map_err(map_sqlx_error)?;

        for entry in &server.env {
            let encoded_value = encode_env_value(&entry.value);

            sqlx::query("INSERT INTO mcp_server_env (server_id, key, value) VALUES (?, ?, ?)")
                .bind(server.id)
                .bind(&entry.key)
                .bind(&encoded_value)
                .execute(&self.pool)
                .await
                .map_err(map_sqlx_error)?;
        }

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
            .map_err(map_sqlx_error)?;

        Ok(())
    }

    async fn update_last_connected(&self, id: i64) -> Result<(), McpRepositoryError> {
        let result =
            sqlx::query("UPDATE mcp_servers SET last_connected_at = datetime('now') WHERE id = ?")
                .bind(id)
                .execute(&self.pool)
                .await
                .map_err(map_sqlx_error)?;

        if result.rows_affected() == 0 {
            return Err(McpRepositoryError::NotFound(id.to_string()));
        }

        Ok(())
    }
}

impl SqliteMcpRepository {
    /// Fetch and decode environment variables for a server.
    async fn fetch_env(&self, server_id: i64) -> Result<Vec<McpEnvEntry>, McpRepositoryError> {
        let rows = sqlx::query_as::<_, EnvRow>(
            "SELECT key, value FROM mcp_server_env WHERE server_id = ?",
        )
        .bind(server_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let mut env = Vec::with_capacity(rows.len());
        for row in rows {
            let decoded_value = decode_env_value(&row.value)?;
            env.push(McpEnvEntry::new(row.key, decoded_value));
        }

        Ok(env)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    async fn setup_test_db() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();

        // Create the mcp_servers table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS mcp_servers (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE,
                type TEXT NOT NULL CHECK (type IN ('stdio', 'sse')),
                enabled INTEGER NOT NULL DEFAULT 1,
                auto_start INTEGER NOT NULL DEFAULT 0,
                command TEXT,
                resolved_path_cache TEXT,
                args TEXT,
                cwd TEXT,
                path_extra TEXT,
                url TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                last_connected_at TEXT,
                is_valid INTEGER NOT NULL DEFAULT 0,
                last_error TEXT
            )
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();

        // Create the mcp_server_env table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS mcp_server_env (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                server_id INTEGER NOT NULL,
                key TEXT NOT NULL,
                value TEXT NOT NULL,
                FOREIGN KEY (server_id) REFERENCES mcp_servers(id) ON DELETE CASCADE,
                UNIQUE(server_id, key)
            )
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();

        pool
    }

    #[tokio::test]
    async fn test_insert_and_get_by_id() {
        let pool = setup_test_db().await;
        let repo = SqliteMcpRepository::new(pool);

        let new_server = NewMcpServer::new_stdio(
            "test-server",
            "npx",
            vec!["-y".to_string(), "mcp".to_string()],
            None,
        )
        .with_env("API_KEY", "secret123");

        let server = repo.insert(new_server).await.unwrap();

        assert_eq!(server.name, "test-server");
        assert_eq!(server.server_type, McpServerType::Stdio);
        assert_eq!(server.config.command, Some("npx".to_string()));
        assert_eq!(server.env.len(), 1);
        assert_eq!(server.env[0].key, "API_KEY");
        assert_eq!(server.env[0].value, "secret123");

        // Fetch by ID
        let fetched = repo.get_by_id(server.id).await.unwrap();
        assert_eq!(fetched.name, "test-server");
        assert_eq!(fetched.env[0].value, "secret123");
    }

    #[tokio::test]
    async fn test_get_by_name() {
        let pool = setup_test_db().await;
        let repo = SqliteMcpRepository::new(pool);

        let new_server =
            NewMcpServer::new_stdio("my-mcp", "node", vec!["server.js".to_string()], None);
        let _ = repo.insert(new_server).await.unwrap();

        let fetched = repo.get_by_name("my-mcp").await.unwrap();
        assert_eq!(fetched.name, "my-mcp");
    }

    #[tokio::test]
    async fn test_list_servers() {
        let pool = setup_test_db().await;
        let repo = SqliteMcpRepository::new(pool);

        repo.insert(NewMcpServer::new_stdio("server-a", "cmd", vec![], None))
            .await
            .unwrap();
        repo.insert(NewMcpServer::new_stdio("server-b", "cmd", vec![], None))
            .await
            .unwrap();

        let servers = repo.list().await.unwrap();
        assert_eq!(servers.len(), 2);
        // Should be ordered by name
        assert_eq!(servers[0].name, "server-a");
        assert_eq!(servers[1].name, "server-b");
    }

    #[tokio::test]
    async fn test_update_server() {
        let pool = setup_test_db().await;
        let repo = SqliteMcpRepository::new(pool);

        let new_server = NewMcpServer::new_stdio("updatable", "old-cmd", vec![], None)
            .with_env("KEY", "old-value");
        let mut server = repo.insert(new_server).await.unwrap();

        // Modify the server
        server.config.command = Some("new-cmd".to_string());
        server.env = vec![McpEnvEntry::new("KEY", "new-value")];
        server.enabled = false;

        repo.update(&server).await.unwrap();

        let fetched = repo.get_by_id(server.id).await.unwrap();
        assert_eq!(fetched.config.command, Some("new-cmd".to_string()));
        assert_eq!(fetched.env[0].value, "new-value");
        assert!(!fetched.enabled);
    }

    #[tokio::test]
    async fn test_delete_server() {
        let pool = setup_test_db().await;
        let repo = SqliteMcpRepository::new(pool);

        let new_server = NewMcpServer::new_stdio("deletable", "cmd", vec![], None);
        let server = repo.insert(new_server).await.unwrap();
        let id = server.id;

        repo.delete(id).await.unwrap();

        let result = repo.get_by_id(id).await;
        assert!(matches!(result, Err(McpRepositoryError::NotFound(_))));
    }

    #[tokio::test]
    async fn test_conflict_on_duplicate_name() {
        let pool = setup_test_db().await;
        let repo = SqliteMcpRepository::new(pool);

        repo.insert(NewMcpServer::new_stdio("unique-name", "cmd", vec![], None))
            .await
            .unwrap();

        let result = repo
            .insert(NewMcpServer::new_stdio("unique-name", "cmd", vec![], None))
            .await;

        assert!(matches!(result, Err(McpRepositoryError::Conflict(_))));
    }

    #[tokio::test]
    async fn test_sse_server() {
        let pool = setup_test_db().await;
        let repo = SqliteMcpRepository::new(pool);

        let new_server = NewMcpServer::new_sse("sse-server", "http://localhost:3001/sse");
        let server = repo.insert(new_server).await.unwrap();

        assert_eq!(server.server_type, McpServerType::Sse);
        assert_eq!(
            server.config.url,
            Some("http://localhost:3001/sse".to_string())
        );
    }

    #[tokio::test]
    async fn test_update_last_connected() {
        let pool = setup_test_db().await;
        let repo = SqliteMcpRepository::new(pool);

        let new_server = NewMcpServer::new_stdio("connectable", "cmd", vec![], None);
        let server = repo.insert(new_server).await.unwrap();

        assert!(server.last_connected_at.is_none());

        repo.update_last_connected(server.id).await.unwrap();

        let fetched = repo.get_by_id(server.id).await.unwrap();
        assert!(fetched.last_connected_at.is_some());
    }
}
