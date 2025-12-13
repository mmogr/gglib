//! Settings repository trait definition.
//!
//! This port defines the interface for application settings persistence.
//! Implementations handle all storage details internally.

use async_trait::async_trait;

use super::RepositoryError;
use crate::settings::Settings;

/// Repository for application settings persistence.
///
/// This trait defines operations for storing and retrieving the application
/// settings as a whole. The implementation handles serialization.
///
/// # Design Rules
///
/// - No `sqlx` types in signatures
/// - Works with domain `Settings` type directly
/// - Implementation handles JSON serialization internally
#[async_trait]
pub trait SettingsRepository: Send + Sync {
    /// Load application settings.
    ///
    /// Returns default settings if none are stored.
    async fn load(&self) -> Result<Settings, RepositoryError>;

    /// Save application settings.
    async fn save(&self, settings: &Settings) -> Result<(), RepositoryError>;
}
