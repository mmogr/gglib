//! Settings repository trait definition.
//!
//! This port defines the interface for key-value settings persistence.
//! Implementations must handle all storage details internally.

use async_trait::async_trait;

use super::RepositoryError;

/// Repository for key-value settings persistence.
///
/// This trait defines operations for storing and retrieving settings.
/// Settings are simple string key-value pairs.
///
/// # Design Rules
///
/// - No `sqlx` types in signatures
/// - Simple key-value interface
/// - All values stored as strings (serialization handled by service layer)
#[async_trait]
pub trait SettingsRepository: Send + Sync {
    /// Get a setting value by key.
    ///
    /// Returns `Ok(None)` if the key doesn't exist.
    async fn get(&self, key: &str) -> Result<Option<String>, RepositoryError>;

    /// Set a setting value.
    ///
    /// Creates the key if it doesn't exist, or updates if it does (upsert).
    async fn set(&self, key: &str, value: &str) -> Result<(), RepositoryError>;

    /// Delete a setting by key.
    ///
    /// Returns `Ok(())` even if the key doesn't exist (idempotent).
    async fn delete(&self, key: &str) -> Result<(), RepositoryError>;

    /// List all settings as key-value pairs.
    async fn list(&self) -> Result<Vec<(String, String)>, RepositoryError>;
}
