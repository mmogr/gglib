//! Defect-window repository port definition.
//!
//! Persists the *unacted* per-model defect windows so a daemon restart
//! resumes the count instead of zeroing it. Implementations live in
//! `gglib-db`; this trait contains only domain types.
//!
//! Staleness is answered at load, not at store. The store keeps whatever it
//! was given, including rows from other llama.cpp releases; the reader
//! decays by wall-clock age and rejects foreign builds. Keeping that policy
//! out of the repository is what lets it be unit-tested with no database —
//! see [`crate::domain::defects::decay`].

use async_trait::async_trait;

use super::RepositoryError;
use crate::domain::defects::PersistedDefectWindow;

/// Repository interface for persisted defect windows.
#[async_trait]
pub trait DefectWindowRepositoryPort: Send + Sync {
    /// Every persisted window, all builds included — build filtering and
    /// decay are the reader's policy, not the store's.
    async fn load_all(&self) -> Result<Vec<PersistedDefectWindow>, RepositoryError>;

    /// Upsert these rows in one transaction — a flush is one fact about one
    /// moment and must not half-land.
    async fn upsert_many(&self, rows: &[PersistedDefectWindow]) -> Result<(), RepositoryError>;

    /// Delete the named rows in one transaction (stale-build discards, and
    /// windows decayed to nothing).
    async fn delete(&self, model_names: &[String]) -> Result<(), RepositoryError>;
}
