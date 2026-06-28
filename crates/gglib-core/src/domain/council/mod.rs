#![doc = include_str!("README.md")]
pub mod events;
pub mod role_catalog;
pub mod run;
pub mod task_graph;

pub use events::{AgentStance, ApprovalKind, CouncilEvent, StanceOutcome};
pub use role_catalog::{RoleCatalog, RoleId, RoleSpec};
pub use run::{CouncilRun, CouncilRunEvent, CouncilRunStatus};
pub use task_graph::{
    DebateAgent, DebateConfig, DebateJudgeConfig, HitlMode, MAX_DEPTH, MAX_NODES, MAX_TOTAL_NODES,
    NodeId, NodeStatus, TaskGraph, TaskGraphError, TaskNode, TaskNodeKind,
};
