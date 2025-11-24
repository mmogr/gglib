//! Database operations for GGUF model management.
//!
//! This module handles all SQLite database interactions including:
//! - Database setup and schema management
//! - Adding models to the database
//! - Querying and listing stored models
//! - Model metadata serialization and storage

use crate::models::Gguf;
use anyhow::{Result, anyhow};
use sqlx::{Row, SqlitePool, sqlite::SqliteConnectOptions};
use std::path::Path;
use thiserror::Error;

/// Domain-specific errors for model storage operations.
#[derive(Debug, Error)]
pub enum ModelStoreError {
    #[error(
        "Model '{model_name}' is already tracked (id {existing_id}) for file {file_path}. Remove it before downloading again."
    )]
    DuplicateModel {
        model_name: String,
        file_path: String,
        existing_id: u32,
    },
}

/// Sets up the SQLite database connection and ensures the database schema exists.
///
/// This function:
/// 1. Creates the data directory if it doesn't exist
/// 2. Establishes a connection to the SQLite database file
/// 3. Creates the models table if it doesn't exist
/// 4. Runs any necessary schema migrations
///
/// # Returns
///
/// Returns a `Result<SqlitePool>` containing the database connection pool on success,
/// or an error if database setup fails.
///
/// # Errors
///
/// This function will return an error if:
/// - The data directory cannot be created
/// - The database file cannot be opened or created
/// - The table creation SQL fails to execute
///
/// # Examples
///
/// ```rust,no_run
/// use gglib::database;
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     let pool = database::setup_database().await?;
///     Ok(())
/// }
/// ```
pub async fn setup_database() -> Result<SqlitePool> {
    // Get the absolute path to the database file
    let db_path = crate::utils::paths::get_database_path()?;

    let pool: sqlx::Pool<sqlx::Sqlite> = SqlitePool::connect_with(
        SqliteConnectOptions::new()
            .filename(&db_path)
            .create_if_missing(true),
    )
    .await?;

    // Create the models table with enhanced metadata fields
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS models (
            id INTEGER PRIMARY KEY, 
            name TEXT NOT NULL, 
            file_path TEXT NOT NULL, 
            param_count_b REAL NOT NULL,
            architecture TEXT,
            quantization TEXT,
            context_length INTEGER,
            metadata TEXT,
            added_at TEXT NOT NULL,
            hf_repo_id TEXT,
            hf_commit_sha TEXT,
            hf_filename TEXT,
            download_date TEXT,
            last_update_check TEXT,
            tags TEXT DEFAULT '[]'
            )",
    )
    .execute(&pool)
    .await?;

    // Prevent duplicate entries for the same on-disk GGUF file.
    sqlx::query("CREATE UNIQUE INDEX IF NOT EXISTS idx_models_file_path ON models(file_path)")
        .execute(&pool)
        .await?;

    // Create chat conversations table for chat history
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS chat_conversations (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            title TEXT NOT NULL,
            model_id INTEGER,
            system_prompt TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (model_id) REFERENCES models(id) ON DELETE SET NULL
        )",
    )
    .execute(&pool)
    .await?;

    ensure_column_exists(&pool, "chat_conversations", "system_prompt", "TEXT").await?;

    // Create chat messages table for storing conversation messages
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS chat_messages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            conversation_id INTEGER NOT NULL,
            role TEXT NOT NULL CHECK(role IN ('system', 'user', 'assistant')),
            content TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (conversation_id) REFERENCES chat_conversations(id) ON DELETE CASCADE
        )",
    )
    .execute(&pool)
    .await?;

    // Create index on conversation_id for faster message queries
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_messages_conversation 
         ON chat_messages(conversation_id)",
    )
    .execute(&pool)
    .await?;

    // Initialize settings table
    crate::services::settings::init_settings_table(&pool).await?;

    Ok(pool)
}

fn normalized_file_path_string(path: &Path) -> String {
    std::fs::canonicalize(path)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| path.to_string_lossy().to_string())
}

struct ExistingModelRecord {
    id: u32,
    name: String,
    file_path: String,
}

async fn find_existing_model_by_path(
    pool: &SqlitePool,
    file_path: &str,
) -> Result<Option<ExistingModelRecord>> {
    if let Some(row) =
        sqlx::query("SELECT id, name, file_path FROM models WHERE file_path = ? LIMIT 1")
            .bind(file_path)
            .fetch_optional(pool)
            .await?
    {
        let id: i64 = row.get("id");
        let name: String = row.get("name");
        let path: String = row.get("file_path");
        return Ok(Some(ExistingModelRecord {
            id: id as u32,
            name,
            file_path: path,
        }));
    }

    Ok(None)
}

async fn ensure_column_exists(
    pool: &SqlitePool,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<()> {
    // Validate table name against a whitelist
    let valid_tables = ["chat_conversations", "chat_messages", "models"];
    if !valid_tables.contains(&table) {
        return Err(anyhow::anyhow!("Invalid table name: {}", table));
    }

    // Validate column name contains only alphanumeric and underscore
    if !column.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Err(anyhow::anyhow!("Invalid column name: {}", column));
    }

    // Validate column definition starts with a valid SQL type
    let valid_definitions = ["TEXT", "INTEGER", "REAL", "BLOB"];
    if !valid_definitions
        .iter()
        .any(|def| definition.to_uppercase().starts_with(def))
    {
        return Err(anyhow::anyhow!("Invalid column definition: {}", definition));
    }

    // Use double quotes to safely quote the table name identifier
    let pragma_query = format!(
        "SELECT COUNT(*) FROM pragma_table_info(\"{}\") WHERE name = ?",
        table
    );
    let column_exists: i64 = sqlx::query_scalar(&pragma_query)
        .bind(column)
        .fetch_one(pool)
        .await?;

    if column_exists == 0 {
        // Use double quotes to safely quote identifiers
        let alter_stmt = format!(
            "ALTER TABLE \"{}\" ADD COLUMN \"{}\" {}",
            table, column, definition
        );

        if let Err(err) = sqlx::query(&alter_stmt).execute(pool).await {
            let is_duplicate_column = matches!(
                &err,
                sqlx::Error::Database(db_err)
                    if db_err.message().contains("duplicate column name")
            );

            if is_duplicate_column {
                return Ok(());
            }

            return Err(err.into());
        }
    }

    Ok(())
}

/// Adds a new GGUF model to the database.
///
/// This function inserts a model record into the SQLite database with all
/// the model's metadata including name, file path, parameter count,
/// architecture, quantization, context length, and additional metadata.
///
/// # Arguments
///
/// * `pool` - A reference to the SQLite connection pool
/// * `model` - A reference to the `Gguf` model to be added to the database
///
/// # Returns
///
/// Returns `Result<()>` indicating success or failure of the database insertion.
///
/// # Errors
///
/// This function will return an error if:
/// - The database connection fails
/// - The SQL insertion query fails
/// - There are database constraint violations (e.g., duplicate primary keys)
///
/// # Examples
///
/// ```rust,no_run
/// use gglib::{database, Gguf};
/// use std::path::PathBuf;
/// use chrono::Utc;
/// use std::collections::HashMap;
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     let pool = database::setup_database().await?;
///     let model = Gguf {
///         id: None,
///         name: "test".to_string(),
///         file_path: PathBuf::from("/test.gguf"),
///         param_count_b: 1.0,
///         architecture: Some("llama".to_string()),
///         quantization: Some("Q4_0".to_string()),
///         context_length: Some(4096),
///         metadata: HashMap::new(),
///         added_at: Utc::now(),
///         hf_repo_id: None,
///         hf_commit_sha: None,
///         hf_filename: None,
///         download_date: None,
///         last_update_check: None,
///         tags: Vec::new(),
///     };
///     database::add_model(&pool, &model).await?;
///     Ok(())
/// }
/// ```
pub async fn add_model(pool: &SqlitePool, model: &Gguf) -> Result<()> {
    // Serialize metadata HashMap to JSON string
    let metadata_json = serde_json::to_string(&model.metadata)?;
    let file_path_string = normalized_file_path_string(&model.file_path);

    if let Some(existing) = find_existing_model_by_path(pool, &file_path_string).await? {
        return Err(ModelStoreError::DuplicateModel {
            model_name: existing.name,
            file_path: existing.file_path,
            existing_id: existing.id,
        }
        .into());
    }

    sqlx::query("INSERT INTO models (name, file_path, param_count_b, architecture, quantization, context_length, metadata, added_at, hf_repo_id, hf_commit_sha, hf_filename, download_date, last_update_check) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
        .bind(&model.name)
        .bind(&file_path_string)
        .bind(model.param_count_b)
        .bind(&model.architecture)
        .bind(&model.quantization)
        .bind(model.context_length.map(|c| c as i64))
        .bind(&metadata_json)
        .bind(model.added_at.to_string())
        .bind(&model.hf_repo_id)
        .bind(&model.hf_commit_sha)
        .bind(&model.hf_filename)
        .bind(model.download_date.as_ref().map(|d| d.to_string()))
        .bind(model.last_update_check.as_ref().map(|d| d.to_string()))
        .execute(pool)
        .await?;

    Ok(())
}

/// Retrieves all GGUF models from the database.
///
/// This function queries the database and returns a vector containing
/// all stored GGUF models with their metadata.
///
/// # Arguments
///
/// * `pool` - A reference to the SQLite connection pool
///
/// # Returns
///
/// Returns `Result<Vec<Gguf>>` containing all models in the database,
/// or an error if the query fails.
///
/// # Errors
///
/// This function will return an error if:
/// - The database connection fails
/// - The SQL query fails
/// - Data deserialization fails
///
/// # Examples
///
/// ```rust,no_run
/// use gglib::database;
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     let pool = database::setup_database().await?;
///     let models = database::list_models(&pool).await?;
///     for model in models {
///         println!("{}: {}", model.name, model.file_path.display());
///     }
///     Ok(())
/// }
/// ```
pub async fn list_models(pool: &SqlitePool) -> Result<Vec<Gguf>> {
    let models = sqlx::query_as::<_, Gguf>("SELECT id, name, file_path, param_count_b, architecture, quantization, context_length, metadata, added_at, hf_repo_id, hf_commit_sha, hf_filename, download_date, last_update_check, tags FROM models ORDER BY added_at DESC")
        .fetch_all(pool)
        .await?;

    Ok(models)
}

/// Find models by name (partial match for user convenience).
///
/// This function searches for models where the name contains the provided
/// search term (case-insensitive). This allows users to find models
/// without typing the exact full name.
///
/// # Arguments
///
/// * `pool` - A reference to the SQLite connection pool
/// * `name` - The name or partial name to search for
///
/// # Returns
///
/// Returns `Result<Vec<Gguf>>` containing matching models,
/// or an error if the query fails.
///
/// # Examples
///
/// ```rust,no_run
/// use gglib::database;
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     let pool = database::setup_database().await?;
///     let models = database::find_models_by_name(&pool, "llama").await?;
///     println!("Found {} models matching 'llama'", models.len());
///     Ok(())
/// }
/// ```
pub async fn find_models_by_name(pool: &SqlitePool, name: &str) -> Result<Vec<Gguf>> {
    let models = sqlx::query_as::<_, Gguf>("SELECT id, name, file_path, param_count_b, architecture, quantization, context_length, metadata, added_at, hf_repo_id, hf_commit_sha, hf_filename, download_date, last_update_check, tags FROM models WHERE name LIKE ? ORDER BY added_at DESC")
        .bind(format!("%{name}%"))
        .fetch_all(pool)
        .await?;

    Ok(models)
}

/// Get a model from the database by ID.
///
/// This function retrieves a single model record from the database by its ID.
///
/// # Arguments
///
/// * `pool` - A reference to the SQLite connection pool
/// * `id` - The ID of the model to retrieve
///
/// # Returns
///
/// Returns `Ok(Some(Gguf))` if the model is found, `Ok(None)` if not found,
/// or an error if the database operation fails.
///
/// # Examples
///
/// ```rust,no_run
/// use gglib::services::database;
/// use gglib::models::Gguf;
/// use std::collections::HashMap;
/// use std::path::PathBuf;
/// use chrono::Utc;
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     let pool = database::setup_database().await?;
///     
///     // Add a model to get an ID
///     let model = Gguf {
///         id: None,
///         name: "Example Model".to_string(),
///         file_path: PathBuf::from("/path/to/model.gguf"),
///         param_count_b: 7.0,
///         architecture: Some("llama".to_string()),
///         quantization: Some("Q4_0".to_string()),
///         context_length: Some(4096),
///         metadata: HashMap::new(),
///         added_at: Utc::now(),
///         hf_repo_id: None,
///         hf_commit_sha: None,
///         hf_filename: None,
///         download_date: None,
///         last_update_check: None,
///         tags: Vec::new(),
///     };
///     
///     database::add_model(&pool, &model).await?;
///     
///     // Get the model by ID (assuming ID 1 for first model)
///     let retrieved = database::get_model_by_id(&pool, 1).await?;
///     
///     match retrieved {
///         Some(model) => println!("Found model: {}", model.name),
///         None => println!("Model not found"),
///     }
///     
///     Ok(())
/// }
/// ```
pub async fn get_model_by_id(pool: &SqlitePool, id: u32) -> Result<Option<Gguf>> {
    let model = sqlx::query_as::<_, Gguf>("SELECT id, name, file_path, param_count_b, architecture, quantization, context_length, metadata, added_at, hf_repo_id, hf_commit_sha, hf_filename, download_date, last_update_check, tags FROM models WHERE id = ?")
        .bind(id as i64)
        .fetch_optional(pool)
        .await?;

    Ok(model)
}

/// Update a model in the database.
///
/// This function updates an existing model record in the database with new values.
/// All fields except the ID and added_at timestamp can be updated.
///
/// # Arguments
///
/// * `pool` - A reference to the SQLite connection pool
/// * `id` - The ID of the model to update
/// * `model` - The updated model data
///
/// # Returns
///
/// Returns `Ok(())` if the update succeeds, or an error if the operation fails.
///
/// # Examples
///
/// ```rust,no_run
/// use gglib::services::database;
/// use gglib::models::Gguf;
/// use std::collections::HashMap;
/// use std::path::PathBuf;
/// use chrono::Utc;
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     let pool = database::setup_database().await?;
///     
///     // Add a model first
///     let mut metadata = HashMap::new();
///     metadata.insert("version".to_string(), "1.0".to_string());
///     
///     let original_model = Gguf {
///         id: None,
///         name: "Original Model".to_string(),
///         file_path: PathBuf::from("/path/to/model.gguf"),
///         param_count_b: 7.0,
///         architecture: Some("llama".to_string()),
///         quantization: Some("Q4_0".to_string()),
///         context_length: Some(4096),
///         metadata: metadata.clone(),
///         added_at: Utc::now(),
///         hf_repo_id: None,
///         hf_commit_sha: None,
///         hf_filename: None,
///         download_date: None,
///         last_update_check: None,
///         tags: Vec::new(),
///     };
///     
///     database::add_model(&pool, &original_model).await?;
///     
///     // Update the model
///     metadata.insert("version".to_string(), "2.0".to_string());
///     let updated_model = Gguf {
///         id: Some(1), // Assuming first model gets ID 1
///         name: "Updated Model".to_string(),
///         file_path: PathBuf::from("/path/to/updated_model.gguf"),
///         param_count_b: 13.0,
///         architecture: Some("mistral".to_string()),
///         quantization: Some("Q8_0".to_string()),
///         context_length: Some(8192),
///         metadata,
///         added_at: Utc::now(), // This won't be updated in database
///         hf_repo_id: None,
///         hf_commit_sha: None,
///         hf_filename: None,
///         download_date: None,
///         last_update_check: None,
///         tags: Vec::new(),
///     };
///     
///     database::update_model(&pool, 1, &updated_model).await?;
///     println!("Model updated successfully!");
///     
///     Ok(())
/// }
/// ```
pub async fn update_model(pool: &SqlitePool, id: u32, model: &Gguf) -> Result<()> {
    // Serialize metadata HashMap to JSON string
    let metadata_json = serde_json::to_string(&model.metadata)?;

    sqlx::query("UPDATE models SET name = ?, file_path = ?, param_count_b = ?, architecture = ?, quantization = ?, context_length = ?, metadata = ?, hf_repo_id = ?, hf_commit_sha = ?, hf_filename = ?, download_date = ?, last_update_check = ? WHERE id = ?")
        .bind(&model.name)
        .bind(model.file_path.to_string_lossy().as_ref())
        .bind(model.param_count_b)
        .bind(&model.architecture)
        .bind(&model.quantization)
        .bind(model.context_length.map(|c| c as i64))
        .bind(&metadata_json)
        .bind(&model.hf_repo_id)
        .bind(&model.hf_commit_sha)
        .bind(&model.hf_filename)
        .bind(model.download_date.as_ref().map(|dt| dt.to_string()))
        .bind(model.last_update_check.as_ref().map(|dt| dt.to_string()))
        .bind(id as i64)
        .execute(pool)
        .await?;

    Ok(())
}

/// Remove a model from the database by exact name match.
///
/// This function removes a model record from the database. It only removes
/// the database entry - the actual model file is left untouched on disk.
///
/// # Arguments
///
/// * `pool` - A reference to the SQLite connection pool
/// * `name` - The exact name of the model to remove
///
/// # Returns
///
/// Returns `Result<()>` indicating success or failure.
///
/// # Errors
///
/// This function will return an error if:
/// - The database query fails
/// - No model with the specified name exists
///
/// # Examples
///
/// ```rust,no_run
/// use gglib::database;
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     let pool = database::setup_database().await?;
///     database::remove_model(&pool, "my-model").await?;
///     println!("Model removed successfully");
///     Ok(())
/// }
/// ```
pub async fn find_model_by_identifier(pool: &SqlitePool, identifier: &str) -> Result<Option<Gguf>> {
    // First try to find by exact name match
    let exact_match = sqlx::query_as::<_, Gguf>("SELECT id, name, file_path, param_count_b, architecture, quantization, context_length, metadata, added_at, hf_repo_id, hf_commit_sha, hf_filename, download_date, last_update_check, tags FROM models WHERE name = ?")
        .bind(identifier)
        .fetch_optional(pool)
        .await?;

    if let Some(model) = exact_match {
        return Ok(Some(model));
    }

    // If no exact match, try to find by ID if the identifier is numeric
    if let Ok(id) = identifier.parse::<i64>() {
        let id_match = sqlx::query_as::<_, Gguf>("SELECT id, name, file_path, param_count_b, architecture, quantization, context_length, metadata, added_at, hf_repo_id, hf_commit_sha, hf_filename, download_date, last_update_check, tags FROM models WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await?;

        if let Some(model) = id_match {
            return Ok(Some(model));
        }
    }

    Ok(None)
}

pub async fn remove_model(pool: &SqlitePool, name: &str) -> Result<()> {
    let result = sqlx::query("DELETE FROM models WHERE name = ?")
        .bind(name)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(anyhow::anyhow!("No model found with name: {}", name));
    }

    Ok(())
}

/// Remove a model from the database by ID.
///
/// Prefer this over name-based deletion whenever a stable identifier is available.
pub async fn remove_model_by_id(pool: &SqlitePool, id: u32) -> Result<()> {
    let result = sqlx::query("DELETE FROM models WHERE id = ?")
        .bind(id as i64)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(anyhow!("No model found with id: {}", id));
    }

    Ok(())
}

// Implement custom query trait for sqlx - used by proxy manager
impl<'r> sqlx::FromRow<'r, sqlx::sqlite::SqliteRow> for crate::models::Gguf {
    fn from_row(row: &'r sqlx::sqlite::SqliteRow) -> std::result::Result<Self, sqlx::Error> {
        use chrono::{DateTime, NaiveDateTime, Utc};
        use sqlx::Row;

        // Helper to convert i64 to u64 for context_length
        let context_length: Option<u64> = row
            .try_get::<Option<i64>, _>("context_length")?
            .map(|v| v as u64);

        // Helper to parse datetime strings that may have "UTC" suffix
        let parse_datetime = |row: &sqlx::sqlite::SqliteRow,
                              column: &str|
         -> std::result::Result<Option<DateTime<Utc>>, sqlx::Error> {
            let datetime_str: Option<String> = row.try_get(column)?;
            match datetime_str {
                Some(s) => {
                    // Remove " UTC" suffix if present and parse
                    let trimmed = s.trim_end_matches(" UTC");
                    NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%d %H:%M:%S%.f")
                        .map(|dt| Some(DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc)))
                        .map_err(|e| sqlx::Error::Decode(Box::new(e)))
                }
                None => Ok(None),
            }
        };

        Ok(crate::models::Gguf {
            id: row.try_get::<Option<i32>, _>("id")?.map(|v| v as u32),
            name: row.try_get("name")?,
            file_path: row.try_get::<String, _>("file_path")?.into(),
            param_count_b: row.try_get("param_count_b")?,
            architecture: row.try_get("architecture")?,
            quantization: row.try_get("quantization")?,
            context_length,
            metadata: serde_json::from_str(&row.try_get::<String, _>("metadata")?)
                .unwrap_or_default(),
            added_at: parse_datetime(row, "added_at")?.unwrap_or_else(Utc::now),
            hf_repo_id: row.try_get("hf_repo_id")?,
            hf_commit_sha: row.try_get("hf_commit_sha")?,
            hf_filename: row.try_get("hf_filename")?,
            download_date: parse_datetime(row, "download_date")?,
            last_update_check: parse_datetime(row, "last_update_check")?,
            tags: serde_json::from_str(&row.try_get::<String, _>("tags")?).unwrap_or_default(),
        })
    }
}

// Tag management operations

/// Get all unique tags used across all models
pub async fn list_tags(pool: &SqlitePool) -> Result<Vec<String>> {
    let rows =
        sqlx::query("SELECT DISTINCT tags FROM models WHERE tags IS NOT NULL AND tags != '[]'")
            .fetch_all(pool)
            .await?;

    let mut all_tags = std::collections::HashSet::new();
    for row in rows {
        let tags_json: String = row.get("tags");
        if let Ok(tags) = serde_json::from_str::<Vec<String>>(&tags_json) {
            for tag in tags {
                all_tags.insert(tag);
            }
        }
    }

    let mut tags: Vec<String> = all_tags.into_iter().collect();
    tags.sort();
    Ok(tags)
}

/// Add a tag to a model
pub async fn add_model_tag(pool: &SqlitePool, model_id: u32, tag: String) -> Result<()> {
    // Get current tags
    let row = sqlx::query("SELECT tags FROM models WHERE id = ?")
        .bind(model_id as i64)
        .fetch_one(pool)
        .await?;

    let tags_json: String = row.get("tags");
    let mut tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();

    // Add tag if not already present
    if !tags.contains(&tag) {
        tags.push(tag);
        tags.sort();

        let updated_tags = serde_json::to_string(&tags)?;
        sqlx::query("UPDATE models SET tags = ? WHERE id = ?")
            .bind(updated_tags)
            .bind(model_id as i64)
            .execute(pool)
            .await?;
    }

    Ok(())
}

/// Remove a tag from a model
pub async fn remove_model_tag(pool: &SqlitePool, model_id: u32, tag: String) -> Result<()> {
    // Get current tags
    let row = sqlx::query("SELECT tags FROM models WHERE id = ?")
        .bind(model_id as i64)
        .fetch_one(pool)
        .await?;

    let tags_json: String = row.get("tags");
    let mut tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();

    // Remove tag
    tags.retain(|t| t != &tag);

    let updated_tags = serde_json::to_string(&tags)?;
    sqlx::query("UPDATE models SET tags = ? WHERE id = ?")
        .bind(updated_tags)
        .bind(model_id as i64)
        .execute(pool)
        .await?;

    Ok(())
}

/// Get all tags for a specific model
pub async fn get_model_tags(pool: &SqlitePool, model_id: u32) -> Result<Vec<String>> {
    let row = sqlx::query("SELECT tags FROM models WHERE id = ?")
        .bind(model_id as i64)
        .fetch_one(pool)
        .await?;

    let tags_json: String = row.get("tags");
    let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();

    Ok(tags)
}

/// Get all model IDs that have a specific tag
#[allow(clippy::collapsible_if)]
pub async fn get_models_by_tag(pool: &SqlitePool, tag: String) -> Result<Vec<u32>> {
    let rows = sqlx::query("SELECT id, tags FROM models")
        .fetch_all(pool)
        .await?;

    let mut model_ids = Vec::new();
    for row in rows {
        let tags_json: String = row.get("tags");
        if let Ok(tags) = serde_json::from_str::<Vec<String>>(&tags_json) {
            if tags.contains(&tag) {
                model_ids.push(row.get::<i64, _>("id") as u32);
            }
        }
    }

    Ok(model_ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Gguf;
    use chrono::Utc;
    use std::collections::HashMap;
    use std::path::PathBuf;

    async fn create_test_pool() -> Result<SqlitePool> {
        // Use in-memory database for testing
        let pool = SqlitePool::connect("sqlite::memory:").await?;

        // Create the table with enhanced metadata fields
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS models (
                id INTEGER PRIMARY KEY, 
                name TEXT NOT NULL, 
                file_path TEXT NOT NULL, 
                param_count_b REAL NOT NULL,
                architecture TEXT,
                quantization TEXT,
                context_length INTEGER,
                metadata TEXT,
                added_at TEXT NOT NULL,
                hf_repo_id TEXT,
                hf_commit_sha TEXT,
                hf_filename TEXT,
                download_date TEXT,
                last_update_check TEXT,
                tags TEXT DEFAULT '[]'
                )",
        )
        .execute(&pool)
        .await?;

        Ok(pool)
    }

    fn create_test_model(name: &str) -> Gguf {
        let mut metadata = HashMap::new();
        metadata.insert("test_key".to_string(), "test_value".to_string());

        let mut model = Gguf::new(
            name.to_string(),
            PathBuf::from(format!("/test/{}.gguf", name)),
            7.0,
            Utc::now(),
        );
        model.architecture = Some("llama".to_string());
        model.quantization = Some("Q4_0".to_string());
        model.context_length = Some(4096);
        model.metadata = metadata;
        model
    }

    #[tokio::test]
    async fn test_setup_database() {
        let pool = create_test_pool().await;
        assert!(pool.is_ok());
    }

    #[tokio::test]
    async fn test_add_model() {
        let pool = create_test_pool().await.unwrap();
        let model = create_test_model("test_model");

        let result = add_model(&pool, &model).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_add_model_rejects_duplicates() {
        let pool = create_test_pool().await.unwrap();
        let model = create_test_model("dup_model");

        add_model(&pool, &model).await.unwrap();
        let err = add_model(&pool, &model)
            .await
            .expect_err("expected duplicate error");

        let duplicate = err.downcast::<ModelStoreError>().unwrap();
        match duplicate {
            ModelStoreError::DuplicateModel { file_path, .. } => {
                assert!(file_path.contains("dup_model"));
            }
        }
    }

    #[tokio::test]
    async fn test_list_models() {
        let pool = create_test_pool().await.unwrap();
        let model1 = create_test_model("model1");
        let model2 = create_test_model("model2");

        add_model(&pool, &model1).await.unwrap();
        add_model(&pool, &model2).await.unwrap();

        let models = list_models(&pool).await.unwrap();
        assert_eq!(models.len(), 2);

        // Should be ordered by added_at DESC
        assert_eq!(models[1].name, "model1");
        assert_eq!(models[0].name, "model2");
    }

    #[tokio::test]
    async fn test_find_models_by_name() {
        let pool = create_test_pool().await.unwrap();
        let model1 = create_test_model("llama-7b-chat");
        let model2 = create_test_model("mistral-7b");
        let model3 = create_test_model("llama-13b");

        add_model(&pool, &model1).await.unwrap();
        add_model(&pool, &model2).await.unwrap();
        add_model(&pool, &model3).await.unwrap();

        let llama_models = find_models_by_name(&pool, "llama").await.unwrap();
        assert_eq!(llama_models.len(), 2);

        let mistral_models = find_models_by_name(&pool, "mistral").await.unwrap();
        assert_eq!(mistral_models.len(), 1);
        assert_eq!(mistral_models[0].name, "mistral-7b");
    }

    #[tokio::test]
    async fn test_remove_model() {
        let pool = create_test_pool().await.unwrap();
        let model = create_test_model("to_be_removed");

        add_model(&pool, &model).await.unwrap();

        let models_before = list_models(&pool).await.unwrap();
        assert_eq!(models_before.len(), 1);

        let result = remove_model(&pool, "to_be_removed").await;
        assert!(result.is_ok());

        let models_after = list_models(&pool).await.unwrap();
        assert_eq!(models_after.len(), 0);
    }

    #[tokio::test]
    async fn test_remove_model_by_id() {
        let pool = create_test_pool().await.unwrap();
        let model = create_test_model("remove_by_id");

        add_model(&pool, &model).await.unwrap();
        let models_before = list_models(&pool).await.unwrap();
        let model_id = models_before[0].id.unwrap();

        let result = remove_model_by_id(&pool, model_id).await;
        assert!(result.is_ok());

        let models_after = list_models(&pool).await.unwrap();
        assert!(models_after.is_empty());
    }

    #[tokio::test]
    async fn test_remove_nonexistent_model() {
        let pool = create_test_pool().await.unwrap();

        let result = remove_model(&pool, "nonexistent").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No model found"));
    }

    #[tokio::test]
    async fn test_remove_model_by_id_missing() {
        let pool = create_test_pool().await.unwrap();
        let result = remove_model_by_id(&pool, 42).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No model found"));
    }

    #[tokio::test]
    async fn test_model_with_metadata_serialization() {
        let pool = create_test_pool().await.unwrap();
        let mut metadata = HashMap::new();
        metadata.insert("general.name".to_string(), "Test Model".to_string());
        metadata.insert("llama.context_length".to_string(), "4096".to_string());
        metadata.insert("general.architecture".to_string(), "llama".to_string());

        let mut model = Gguf::new(
            "metadata_test".to_string(),
            PathBuf::from("/test/metadata.gguf"),
            7.0,
            Utc::now(),
        );
        model.architecture = Some("llama".to_string());
        model.quantization = Some("Q4_0".to_string());
        model.context_length = Some(4096);
        model.metadata = metadata;

        add_model(&pool, &model).await.unwrap();

        let retrieved_models = list_models(&pool).await.unwrap();
        assert_eq!(retrieved_models.len(), 1);

        let retrieved_model = &retrieved_models[0];
        assert_eq!(retrieved_model.metadata.len(), 3);
        assert_eq!(
            retrieved_model.metadata.get("general.name"),
            Some(&"Test Model".to_string())
        );
        assert_eq!(
            retrieved_model.metadata.get("llama.context_length"),
            Some(&"4096".to_string())
        );
    }

    #[tokio::test]
    async fn test_model_with_optional_fields() {
        let pool = create_test_pool().await.unwrap();
        let model = Gguf::new(
            "minimal_model".to_string(),
            PathBuf::from("/test/minimal.gguf"),
            1.3,
            Utc::now(),
        );

        add_model(&pool, &model).await.unwrap();

        let retrieved_models = list_models(&pool).await.unwrap();
        assert_eq!(retrieved_models.len(), 1);

        let retrieved_model = &retrieved_models[0];
        assert_eq!(retrieved_model.name, "minimal_model");
        assert_eq!(retrieved_model.architecture, None);
        assert_eq!(retrieved_model.quantization, None);
        assert_eq!(retrieved_model.context_length, None);
        assert!(retrieved_model.metadata.is_empty());
    }

    #[tokio::test]
    async fn test_get_model_by_id_success() {
        let pool = create_test_pool().await.unwrap();
        let model = create_test_model("get_by_id_test");

        add_model(&pool, &model).await.unwrap();

        // Get the added model's ID by listing all models
        let models = list_models(&pool).await.unwrap();
        assert_eq!(models.len(), 1);
        let model_id = models[0].id.unwrap();

        // Test get_model_by_id with valid ID
        let retrieved_model = get_model_by_id(&pool, model_id).await.unwrap();
        assert!(retrieved_model.is_some());

        let retrieved_model = retrieved_model.unwrap();
        assert_eq!(retrieved_model.name, "get_by_id_test");
        assert_eq!(retrieved_model.id, Some(model_id));
        assert_eq!(retrieved_model.param_count_b, 7.0);
    }

    #[tokio::test]
    async fn test_get_model_by_id_not_found() {
        let pool = create_test_pool().await.unwrap();

        // Test get_model_by_id with non-existent ID
        let result = get_model_by_id(&pool, 999).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_get_model_by_id_with_metadata() {
        let pool = create_test_pool().await.unwrap();
        let mut metadata = HashMap::new();
        metadata.insert("general.name".to_string(), "Test Model".to_string());
        metadata.insert("llama.context_length".to_string(), "4096".to_string());

        let mut model = Gguf::new(
            "metadata_get_test".to_string(),
            PathBuf::from("/test/metadata.gguf"),
            13.0,
            Utc::now(),
        );
        model.architecture = Some("llama".to_string());
        model.quantization = Some("Q4_0".to_string());
        model.context_length = Some(4096);
        model.metadata = metadata;

        add_model(&pool, &model).await.unwrap();

        let models = list_models(&pool).await.unwrap();
        let model_id = models[0].id.unwrap();

        let retrieved_model = get_model_by_id(&pool, model_id).await.unwrap().unwrap();
        assert_eq!(retrieved_model.metadata.len(), 2);
        assert_eq!(
            retrieved_model.metadata.get("general.name"),
            Some(&"Test Model".to_string())
        );
        assert_eq!(retrieved_model.architecture, Some("llama".to_string()));
        assert_eq!(retrieved_model.quantization, Some("Q4_0".to_string()));
        assert_eq!(retrieved_model.context_length, Some(4096));
    }

    #[tokio::test]
    async fn test_update_model_success() {
        let pool = create_test_pool().await.unwrap();
        let original_model = create_test_model("update_test_original");

        add_model(&pool, &original_model).await.unwrap();

        // Get the model ID
        let models = list_models(&pool).await.unwrap();
        let model_id = models[0].id.unwrap();

        // Create updated model data
        let mut updated_metadata = HashMap::new();
        updated_metadata.insert("general.updated".to_string(), "true".to_string());

        let mut updated_model = Gguf::new(
            "update_test_modified".to_string(),
            PathBuf::from("/test/updated.gguf"),
            13.0,
            Utc::now(), // This won't be updated in the database
        );
        updated_model.id = Some(model_id);
        updated_model.architecture = Some("llama".to_string());
        updated_model.quantization = Some("Q8_0".to_string());
        updated_model.context_length = Some(8192);
        updated_model.metadata = updated_metadata;

        // Perform the update
        let result = update_model(&pool, model_id, &updated_model).await;
        assert!(result.is_ok());

        // Verify the update
        let retrieved_model = get_model_by_id(&pool, model_id).await.unwrap().unwrap();
        assert_eq!(retrieved_model.name, "update_test_modified");
        assert_eq!(retrieved_model.param_count_b, 13.0);
        assert_eq!(retrieved_model.architecture, Some("llama".to_string()));
        assert_eq!(retrieved_model.quantization, Some("Q8_0".to_string()));
        assert_eq!(retrieved_model.context_length, Some(8192));
        assert_eq!(retrieved_model.metadata.len(), 1);
        assert_eq!(
            retrieved_model.metadata.get("general.updated"),
            Some(&"true".to_string())
        );

        // Verify that the added_at timestamp wasn't changed
        assert_ne!(retrieved_model.added_at, updated_model.added_at);
    }

    #[tokio::test]
    async fn test_update_model_clear_optional_fields() {
        let pool = create_test_pool().await.unwrap();
        let mut original_metadata = HashMap::new();
        original_metadata.insert("key1".to_string(), "value1".to_string());

        let mut original_model = Gguf::new(
            "clear_fields_test".to_string(),
            PathBuf::from("/test/original.gguf"),
            7.0,
            Utc::now(),
        );
        original_model.architecture = Some("llama".to_string());
        original_model.quantization = Some("Q4_0".to_string());
        original_model.context_length = Some(4096);
        original_model.metadata = original_metadata;

        add_model(&pool, &original_model).await.unwrap();

        let models = list_models(&pool).await.unwrap();
        let model_id = models[0].id.unwrap();

        // Update to clear optional fields
        let mut updated_model = Gguf::new(
            "clear_fields_test_updated".to_string(),
            PathBuf::from("/test/cleared.gguf"),
            3.5,
            Utc::now(),
        );
        updated_model.id = Some(model_id);

        update_model(&pool, model_id, &updated_model).await.unwrap();

        let retrieved_model = get_model_by_id(&pool, model_id).await.unwrap().unwrap();
        assert_eq!(retrieved_model.name, "clear_fields_test_updated");
        assert_eq!(retrieved_model.param_count_b, 3.5);
        assert_eq!(retrieved_model.architecture, None);
        assert_eq!(retrieved_model.quantization, None);
        assert_eq!(retrieved_model.context_length, None);
        assert!(retrieved_model.metadata.is_empty());
    }

    #[tokio::test]
    async fn test_update_model_nonexistent_id() {
        let pool = create_test_pool().await.unwrap();
        let dummy_model = create_test_model("dummy");

        // Attempt to update a model with non-existent ID
        let result = update_model(&pool, 999, &dummy_model).await;
        // Note: SQLite won't return an error for UPDATE with WHERE clause that matches no rows
        // It just updates 0 rows, which is considered successful
        assert!(result.is_ok());

        // Verify no models were actually affected
        let models = list_models(&pool).await.unwrap();
        assert_eq!(models.len(), 0);
    }
}
