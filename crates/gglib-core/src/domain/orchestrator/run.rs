//! Persistent run record for orchestrator executions.
//!
//! An [`OrchestratorRun`] is created when `execute()` starts and updated on
//! every state transition.  [`OrchestratorRunEvent`] records every emitted
//! [`crate::domain::orchestrator::events::OrchestratorEvent`] in order so
//! that runs can be inspected and replayed after a process restart.

use serde::{Deserialize, Serialize};

use super::task_graph::{HitlMode, TaskGraph};

// =============================================================================
// OrchestratorRunStatus
// =============================================================================

/// Lifecycle status of a persisted orchestrator run.
///
/// ```text
/// Running ──────────────────────────────────────────► Completed
///   │                                                    ▲
///   ├─ (gate) ──► AwaitingApproval ──(approved)──► Running
///   │
///   └─ (error) ──► Failed
///
/// Running ──(process restart)──► Interrupted
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrchestratorRunStatus {
    /// The run is actively executing.
    Running,
    /// The run is paused waiting for a human-in-the-loop approval decision.
    AwaitingApproval,
    /// The run was interrupted mid-execution by a process restart.
    ///
    /// Interrupted runs can be viewed via `GET /api/orchestrator/runs` but
    /// cannot be automatically resumed in v1 (only `AwaitingApproval` runs
    /// support resume).
    Interrupted,
    /// The run finished successfully.
    Completed,
    /// The run failed with an unrecoverable error.
    Failed,
}

impl std::fmt::Display for OrchestratorRunStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Running => "running",
            Self::AwaitingApproval => "awaiting_approval",
            Self::Interrupted => "interrupted",
            Self::Completed => "completed",
            Self::Failed => "failed",
        };
        f.write_str(s)
    }
}

// =============================================================================
// OrchestratorRun
// =============================================================================

/// A persisted record of a single orchestrator run.
///
/// Created by `execute()` at the start of execution and updated on each state
/// transition.  The `graph_json` field stores the latest serialised graph (with
/// node statuses and compacted outputs) so that interrupted/awaiting runs can
/// be resumed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorRun {
    /// Unique identifier (UUID v4 string).
    pub id: String,
    /// The high-level goal supplied by the user.
    pub goal: String,
    /// Latest serialised [`TaskGraph`] (JSON).
    ///
    /// Updated on plan approval and after each node completes.
    pub graph_json: Option<String>,
    /// Current lifecycle status.
    pub status: OrchestratorRunStatus,
    /// HITL mode used for this run.
    pub hitl_mode: HitlMode,
    /// Optional conversation ID linking this run to a chat session.
    pub conversation_id: Option<i64>,
    /// ISO-8601 creation timestamp.
    pub created_at: String,
    /// ISO-8601 last-updated timestamp.
    pub updated_at: String,
}

impl OrchestratorRun {
    /// Deserialise the stored `graph_json` back into a [`TaskGraph`].
    ///
    /// Returns `None` if no graph has been persisted yet.
    ///
    /// # Errors
    ///
    /// Returns an error if the stored JSON is malformed or does not match the
    /// current schema.
    pub fn graph(&self) -> Option<Result<TaskGraph, serde_json::Error>> {
        self.graph_json.as_deref().map(serde_json::from_str)
    }
}

// =============================================================================
// OrchestratorRunEvent
// =============================================================================

/// A single persisted event record within an orchestrator run.
///
/// Events are appended in sequence order; replaying the full event list
/// reconstructs the run history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorRunEvent {
    /// Foreign-key reference to [`OrchestratorRun::id`].
    pub run_id: String,
    /// 0-based monotonically increasing sequence number within the run.
    pub seq: i64,
    /// Serialised [`crate::domain::orchestrator::events::OrchestratorEvent`] JSON.
    pub event_json: String,
    /// ISO-8601 creation timestamp.
    pub created_at: String,
}
