//! Port trait for persisting orchestrator runs and events.
//!
//! Adapters implement this trait (e.g. [`gglib_db::SqliteCouncilRepository`])
//! and inject it into the executor via [`crate::domain::council::executor::CouncilConfig`].
//! The executor uses it to:
//!
//! - Create a run record at start-up.
//! - Update run status on every state transition.
//! - Append every emitted event to the event log.
//! - Update the serialised graph whenever its state changes.

use async_trait::async_trait;

use crate::domain::council::run::{
    CouncilRun, CouncilRunEvent, CouncilRunStatus,
};
use crate::ports::RepositoryError;

// =============================================================================
// CouncilRepositoryPort
// =============================================================================

/// Persistence operations for orchestrator runs.
///
/// All methods return a [`RepositoryError`] on failure.  The `append_event`
/// method is best-effort in the executor: a storage failure is logged but
/// does NOT abort the run.  All other methods that change run status propagate
/// errors to the executor.
#[async_trait]
pub trait CouncilRepositoryPort: Send + Sync + 'static {
    /// Persist a new run record.
    ///
    /// The run's `id` MUST be unique; callers generate a UUID v4 before
    /// calling this method.
    async fn create_run(&self, run: CouncilRun) -> Result<(), RepositoryError>;

    /// Update the lifecycle status of an existing run.
    async fn update_run_status(
        &self,
        run_id: &str,
        status: CouncilRunStatus,
    ) -> Result<(), RepositoryError>;

    /// Replace the serialised task graph for an existing run.
    ///
    /// Called after plan approval and after each node completes so that the
    /// persisted graph stays up to date for resume purposes.
    async fn update_graph(&self, run_id: &str, graph_json: &str) -> Result<(), RepositoryError>;

    /// Append a single event record to the run's event log.
    async fn append_event(&self, event: CouncilRunEvent) -> Result<(), RepositoryError>;

    /// Retrieve a single run by id.
    ///
    /// Returns `Ok(None)` if the run does not exist.
    async fn get_run(&self, run_id: &str) -> Result<Option<CouncilRun>, RepositoryError>;

    /// List runs optionally filtered by status.
    ///
    /// Results are ordered by `created_at` descending (most recent first).
    async fn list_runs(
        &self,
        status_filter: Option<CouncilRunStatus>,
    ) -> Result<Vec<CouncilRun>, RepositoryError>;

    /// Return all events for a run in sequence order.
    async fn list_events(&self, run_id: &str)
    -> Result<Vec<CouncilRunEvent>, RepositoryError>;

    /// Mark all runs currently in [`CouncilRunStatus::Running`] as
    /// [`CouncilRunStatus::Interrupted`].
    ///
    /// Called once on application boot to handle the case where a process
    /// was killed mid-execution.
    async fn mark_interrupted_runs(&self) -> Result<u64, RepositoryError>;

    /// Delete all events for a run whose `wave_index` is **strictly greater
    /// than** `wave_index`.
    ///
    /// Used by the Phase M rewind endpoint to truncate the event log back to
    /// the state it was in at the end of the specified wave so the run can be
    /// re-executed from that point.
    async fn truncate_events_after_wave(
        &self,
        run_id: &str,
        wave_index: u32,
    ) -> Result<(), RepositoryError>;
}
