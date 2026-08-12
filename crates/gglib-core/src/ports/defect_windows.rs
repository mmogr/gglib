//! Defect-window repository port definition.
//!
//! Persists the auto-tune scheduler's *unacted* defect windows — the
//! per-model evidence it has not yet spent on a signal-driven sweep — so a
//! daemon restart resumes the count instead of zeroing it. Implementations
//! live in `gglib-db`; this trait contains only domain types.
//!
//! What is stored is deliberately the window, not the ledger: acting on a
//! signal advances the scheduler's baseline *before* the sweep runs, so the
//! persisted rows never contain spent evidence and a restart can never
//! re-fire it. Staleness is answered at load, not at store: the reader
//! decays counts by wall-clock age and discards rows recorded against a
//! different llama.cpp release (see `benchmark::auto_tune::restore_plan`).

use async_trait::async_trait;

use super::RepositoryError;
use crate::domain::defects::PersistedDefectWindow;

/// Repository interface for the scheduler's persisted defect windows.
#[async_trait]
pub trait DefectWindowRepositoryPort: Send + Sync {
    /// Every persisted window, all builds included — build filtering and
    /// decay are the reader's policy, not the store's.
    async fn load_all(&self) -> Result<Vec<PersistedDefectWindow>, RepositoryError>;

    /// Upsert these rows in one transaction — a flush is one fact about one
    /// tick and must not half-land.
    async fn upsert_many(&self, rows: &[PersistedDefectWindow]) -> Result<(), RepositoryError>;

    /// Delete the named rows in one transaction (stale-build discards and
    /// windows decayed to nothing).
    async fn delete(&self, model_names: &[String]) -> Result<(), RepositoryError>;
}
