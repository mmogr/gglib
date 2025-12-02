//! Database operations for MCP server configurations.
//!
//! Handles CRUD operations and schema management for MCP servers.

use super::config::{McpServerConfig, McpServerType};
use anyhow::Result;
use sqlx::SqlitePool;
use thiserror::Error;

/// Errors that can occur during MCP database operations.
#[derive(Debug, Error)]
pub enum McpDatabaseError {
    #[error("MCP server not found: {0}")]
    NotFound(String),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Invalid server configuration: {0}")]
    InvalidConfig(String),
}

/// Database operations for MCP servers.
pub struct McpDatabase {
    pool: SqlitePool,
}

impl McpDatabase {
    /// Create a new MCP database instance.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Ensure the MCP tables exist in the database.
    ///
    /// Called during database setup to create tables if needed.
    pub async fn ensure_schema(&self) -> Result<(), McpDatabaseError> {
        // Create MCP servers table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS mcp_servers (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                type TEXT NOT NULL CHECK (type IN ('stdio', 'sse')),
                enabled INTEGER NOT NULL DEFAULT 1,
                auto_start INTEGER NOT NULL DEFAULT 0,
                command TEXT,
                args TEXT,
                cwd TEXT,
                url TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                last_connected_at TEXT
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        // Create environment variables table with encryption support
        // Note: For MVP, values are base64 encoded. Future: proper encryption.
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
        .execute(&self.pool)
        .await?;

        // Index for faster env lookups
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_mcp_env_server ON mcp_server_env(server_id)")
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Add a new MCP server configuration.
    pub async fn add_server(
        &self,
        config: McpServerConfig,
    ) -> Result<McpServerConfig, McpDatabaseError> {
        // Validate config
        match config.server_type {
            McpServerType::Stdio => {
                if config.command.is_none() {
                    return Err(McpDatabaseError::InvalidConfig(
                        "Stdio server requires a command".to_string(),
                    ));
                }
            }
            McpServerType::Sse => {
                if config.url.is_none() {
                    return Err(McpDatabaseError::InvalidConfig(
                        "SSE server requires a URL".to_string(),
                    ));
                }
            }
        }

        let server_type = match config.server_type {
            McpServerType::Stdio => "stdio",
            McpServerType::Sse => "sse",
        };

        let args_json = config
            .args
            .as_ref()
            .map(|a| serde_json::to_string(a).unwrap());

        // Insert the server
        let result = sqlx::query(
            r#"
            INSERT INTO mcp_servers (name, type, enabled, auto_start, command, args, cwd, url)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&config.name)
        .bind(server_type)
        .bind(config.enabled)
        .bind(config.auto_start)
        .bind(&config.command)
        .bind(&args_json)
        .bind(&config.cwd)
        .bind(&config.url)
        .execute(&self.pool)
        .await?;

        let server_id = result.last_insert_rowid();

        // Insert environment variables
        for (key, value) in &config.env {
            // For MVP, use base64 encoding. TODO: proper encryption
            let encoded_value = base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                value.as_bytes(),
            );

            sqlx::query("INSERT INTO mcp_server_env (server_id, key, value) VALUES (?, ?, ?)")
                .bind(server_id)
                .bind(key)
                .bind(&encoded_value)
                .execute(&self.pool)
                .await?;
        }

        // Fetch and return the complete config
        self.get_server(server_id).await
    }

    /// Get a server configuration by ID.
    pub async fn get_server(&self, id: i64) -> Result<McpServerConfig, McpDatabaseError> {
        let row = sqlx::query_as::<_, McpServerRow>(
            r#"
            SELECT id, name, type, enabled, auto_start, command, args, cwd, url, created_at, last_connected_at
            FROM mcp_servers WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| McpDatabaseError::NotFound(id.to_string()))?;

        // Fetch environment variables
        let env = self.get_server_env(id).await?;

        Ok(row.into_config(env))
    }

    /// List all MCP server configurations.
    pub async fn list_servers(&self) -> Result<Vec<McpServerConfig>, McpDatabaseError> {
        let rows = sqlx::query_as::<_, McpServerRow>(
            r#"
            SELECT id, name, type, enabled, auto_start, command, args, cwd, url, created_at, last_connected_at
            FROM mcp_servers ORDER BY name
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let mut configs = Vec::with_capacity(rows.len());
        for row in rows {
            let env = self.get_server_env(row.id).await?;
            configs.push(row.into_config(env));
        }

        Ok(configs)
    }

    /// Update an existing MCP server configuration.
    pub async fn update_server(
        &self,
        id: i64,
        config: McpServerConfig,
    ) -> Result<McpServerConfig, McpDatabaseError> {
        // Validate config exists
        let _ = self.get_server(id).await?;

        let server_type = match config.server_type {
            McpServerType::Stdio => "stdio",
            McpServerType::Sse => "sse",
        };

        let args_json = config
            .args
            .as_ref()
            .map(|a| serde_json::to_string(a).unwrap());

        // Update the server
        sqlx::query(
            r#"
            UPDATE mcp_servers 
            SET name = ?, type = ?, enabled = ?, auto_start = ?, command = ?, args = ?, cwd = ?, url = ?
            WHERE id = ?
            "#,
        )
        .bind(&config.name)
        .bind(server_type)
        .bind(config.enabled)
        .bind(config.auto_start)
        .bind(&config.command)
        .bind(&args_json)
        .bind(&config.cwd)
        .bind(&config.url)
        .bind(id)
        .execute(&self.pool)
        .await?;

        // Update environment variables (delete and re-insert)
        sqlx::query("DELETE FROM mcp_server_env WHERE server_id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

        for (key, value) in &config.env {
            let encoded_value = base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                value.as_bytes(),
            );

            sqlx::query("INSERT INTO mcp_server_env (server_id, key, value) VALUES (?, ?, ?)")
                .bind(id)
                .bind(key)
                .bind(&encoded_value)
                .execute(&self.pool)
                .await?;
        }

        self.get_server(id).await
    }

    /// Remove an MCP server configuration.
    pub async fn remove_server(&self, id: i64) -> Result<(), McpDatabaseError> {
        // Check it exists first
        let _ = self.get_server(id).await?;

        // Env vars are deleted via ON DELETE CASCADE
        sqlx::query("DELETE FROM mcp_servers WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Update the last_connected_at timestamp.
    pub async fn update_last_connected(&self, id: i64) -> Result<(), McpDatabaseError> {
        sqlx::query("UPDATE mcp_servers SET last_connected_at = datetime('now') WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Get environment variables for a server (decoded).
    async fn get_server_env(
        &self,
        server_id: i64,
    ) -> Result<Vec<(String, String)>, McpDatabaseError> {
        let rows = sqlx::query_as::<_, EnvRow>(
            "SELECT key, value FROM mcp_server_env WHERE server_id = ?",
        )
        .bind(server_id)
        .fetch_all(&self.pool)
        .await?;

        let mut env = Vec::with_capacity(rows.len());
        for row in rows {
            // Decode base64 value. TODO: proper decryption
            let decoded =
                base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &row.value)
                    .map_err(|e| {
                        McpDatabaseError::InvalidConfig(format!("Failed to decode env var: {}", e))
                    })?;

            let value = String::from_utf8(decoded).map_err(|e| {
                McpDatabaseError::InvalidConfig(format!("Invalid UTF-8 in env var: {}", e))
            })?;

            env.push((row.key, value));
        }

        Ok(env)
    }
}

/// Internal row type for database queries.
#[derive(sqlx::FromRow)]
struct McpServerRow {
    id: i64,
    name: String,
    #[sqlx(rename = "type")]
    server_type: String,
    enabled: bool,
    auto_start: bool,
    command: Option<String>,
    args: Option<String>,
    cwd: Option<String>,
    url: Option<String>,
    created_at: String,
    last_connected_at: Option<String>,
}

impl McpServerRow {
    fn into_config(self, env: Vec<(String, String)>) -> McpServerConfig {
        let server_type = match self.server_type.as_str() {
            "sse" => McpServerType::Sse,
            _ => McpServerType::Stdio,
        };

        let args: Option<Vec<String>> = self.args.and_then(|a| serde_json::from_str(&a).ok());

        McpServerConfig {
            id: Some(self.id),
            name: self.name,
            server_type,
            enabled: self.enabled,
            auto_start: self.auto_start,
            command: self.command,
            args,
            cwd: self.cwd,
            url: self.url,
            env,
            created_at: Some(self.created_at),
            last_connected_at: self.last_connected_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct EnvRow {
    key: String,
    value: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn setup_test_db() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        let db = McpDatabase::new(pool.clone());
        db.ensure_schema().await.unwrap();
        pool
    }

    #[tokio::test]
    async fn test_add_and_get_server() {
        let pool = setup_test_db().await;
        let db = McpDatabase::new(pool);

        let config =
            McpServerConfig::new_stdio("Test", "npx", vec!["-y".to_string(), "test".to_string()])
                .with_env("API_KEY", "secret123");

        let saved = db.add_server(config).await.unwrap();
        assert!(saved.id.is_some());
        assert_eq!(saved.name, "Test");
        assert_eq!(saved.env.len(), 1);
        assert_eq!(saved.env[0].0, "API_KEY");
        assert_eq!(saved.env[0].1, "secret123");

        let fetched = db.get_server(saved.id.unwrap()).await.unwrap();
        assert_eq!(fetched.name, "Test");
        assert_eq!(fetched.env[0].1, "secret123");
    }

    #[tokio::test]
    async fn test_list_servers() {
        let pool = setup_test_db().await;
        let db = McpDatabase::new(pool);

        db.add_server(McpServerConfig::new_stdio("A", "cmd", vec![]))
            .await
            .unwrap();
        db.add_server(McpServerConfig::new_stdio("B", "cmd", vec![]))
            .await
            .unwrap();

        let servers = db.list_servers().await.unwrap();
        assert_eq!(servers.len(), 2);
    }

    #[tokio::test]
    async fn test_remove_server() {
        let pool = setup_test_db().await;
        let db = McpDatabase::new(pool);

        let saved = db
            .add_server(McpServerConfig::new_stdio("Test", "cmd", vec![]))
            .await
            .unwrap();
        let id = saved.id.unwrap();

        db.remove_server(id).await.unwrap();

        let result = db.get_server(id).await;
        assert!(matches!(result, Err(McpDatabaseError::NotFound(_))));
    }
}
