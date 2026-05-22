//! Orchestrator domain model.
//!
//! This module owns the pure data types that drive the Director/Worker
//! orchestration pattern.  No I/O, no async, no adapter dependencies.
//!
//! # Submodules
//!
//! | Module | Contents |
//! |--------|---------|
//! | [`task_graph`] | [`TaskGraph`], [`TaskNode`], [`NodeId`], [`NodeStatus`], [`HitlMode`], [`TaskGraphError`] |
//! | [`events`] | [`OrchestratorEvent`] — SSE event stream types |
//! | [`run`] | [`OrchestratorRun`], [`OrchestratorRunStatus`], [`OrchestratorRunEvent`] |

pub mod events;
pub mod run;
pub mod task_graph;

pub use events::{ApprovalKind, OrchestratorEvent};
pub use run::{OrchestratorRun, OrchestratorRunEvent, OrchestratorRunStatus};
pub use task_graph::{
    HitlMode, MAX_DEPTH, MAX_NODES, NodeId, NodeStatus, TaskGraph, TaskGraphError, TaskNode,
};
