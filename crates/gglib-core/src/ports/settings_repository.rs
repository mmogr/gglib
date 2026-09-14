//! Settings repository trait definition.
//!
//! This port defines the interface for application settings persistence.
//! Implementations handle all storage details internally.

use async_trait::async_trait;

use super::{CoreError, RepositoryError};
use crate::settings::{Settings, SettingsError};

/// A change to stored settings: given them as they stand, alter them in
/// place or refuse. What [`SettingsRepository::modify`] applies, and free to
/// borrow what it needs for `'a`.
///
/// A named alias rather than the type written out at each use, because a
/// `&mut Settings` elided inside `#[async_trait]` is given a named lifetime,
/// and the closure then no longer accepts a borrow of any lifetime.
pub type SettingsChange<'a> = dyn Fn(&mut Settings) -> Result<(), SettingsError> + Send + Sync + 'a;

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

    /// Read the stored settings, apply `change` to them, and store the
    /// result, with no other write landing in between.
    ///
    /// A partial update needs this. Read, change and save as three calls,
    /// and a write that lands between the read and the save is overwritten
    /// by a record read before it. Settings are written by more than one
    /// process — the daemon, and `gglib config settings set` in a terminal —
    /// so no lock held inside one of them can close that window.
    ///
    /// The default makes exactly those three calls and holds nothing between
    /// them. It is right only for a store no other writer shares, such as an
    /// in-memory test double; a store another process writes overrides it.
    ///
    /// # Errors
    ///
    /// [`CoreError::Settings`] with whatever `change` refused, in which case
    /// nothing is stored, or [`CoreError::Repository`] when the store fails.
    async fn modify(&self, change: &SettingsChange<'_>) -> Result<Settings, CoreError> {
        let mut settings = self.load().await?;
        change(&mut settings)?;
        self.save(&settings).await?;
        Ok(settings)
    }
}
