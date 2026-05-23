//! SSE event types emitted during an orchestrator run.
//!
//! [`OrchestratorEvent`] is the **single source of truth** for the wire format
//! shared by the Axum SSE handler, the CLI consumer, and the TypeScript
//! frontend types.
//!
//! Serialisation uses the same `{"type":"variant_name", ...}` envelope as
//! [`gglib_core::AgentEvent`] and `CouncilEvent` so frontend event handlers
//! stay consistent.
//!
//! # Event lifecycle
//!
//! ```text
//! PlanProposed → [PlanApproved | PlanRejected]
//!   → NodeStarted* → NodeTextDelta* → NodeToolCall* → NodeComplete*
//!   → SynthesisStart → SynthesisTextDelta* → SynthesisComplete
//!   → OrchestratorComplete
//! ```
//!
//! Error paths emit [`OrchestratorEvent::NodeFailed`] or
//! [`OrchestratorEvent::OrchestratorError`] and then the stream closes.

use serde::{Deserialize, Serialize};

use crate::domain::agent::{ToolCall, ToolResult};
use crate::domain::orchestrator::role_catalog::RoleId;

use super::task_graph::{GraphDiff, TaskGraph};

/// Channel capacity for the orchestrator event sender.
///
/// Larger than the per-agent channel to accommodate bursts from multiple
/// concurrent worker nodes plus orchestration bookkeeping events.
pub const ORCHESTRATOR_EVENT_CHANNEL_CAPACITY: usize = 8_192;

// =============================================================================
// ApprovalKind
// =============================================================================

/// Describes what the human-in-the-loop gate is waiting for approval on.
///
/// Carried inside [`OrchestratorEvent::AwaitingApproval`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ApprovalKind {
    /// Approval of the proposed [`TaskGraph`] plan before execution begins.
    Plan,
    /// Approval before a specific worker node starts executing.
    Node {
        /// The id of the node pending approval.
        node_id: String,
    },
    /// Approval before a specific tool call within a worker node.
    Tool {
        /// The worker node that is about to make the tool call.
        node_id: String,
        /// The tool name being called.
        tool_name: String,
    },
    /// Approval before dynamically spawning a sub-team from within a worker node.
    SpawnSubteam {
        /// The worker node that requested the spawn.
        node_id: String,
        /// Roles suggested by the requesting worker for the new team.
        suggested_roles: Vec<String>,
    },
}

// =============================================================================
// OrchestratorEvent
// =============================================================================

/// A single event in an orchestrator execution stream.
///
/// Consumers receive these over SSE (web) or an `mpsc` channel (CLI).
/// Each variant is independently useful — the frontend can render
/// progressively as events arrive without buffering the full stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OrchestratorEvent {
    // ── planning ─────────────────────────────────────────────────────────
    /// The director has produced an initial task plan.
    ///
    /// If `hitl_mode` requires plan approval, the executor immediately
    /// emits [`OrchestratorEvent::AwaitingApproval`] after this and
    /// pauses until the frontend responds.
    PlanProposed { graph: TaskGraph },

    /// A re-planning attempt was triggered (e.g. because the user rejected
    /// the initial plan or a node failed and the director proposes recovery).
    ReplanAttempt {
        /// 1-based retry count.
        attempt: u32,
        /// Human-readable reason for re-planning.
        reason: String,
    },

    /// Warn-only cost estimate emitted immediately after
    /// [`OrchestratorEvent::PlanProposed`].
    ///
    /// Never suppressed, never fatal — the run always proceeds.  The
    /// frontend may display a yellow warning banner when
    /// `est_wall_seconds > 60` or `node_count` exceeds 80 % of the active
    /// [`NodeBudget`] upper bound.
    RunCostEstimate {
        /// Total aggregate node count across all subgraphs.
        node_count: usize,
        /// Rough token estimate (input + output) for the entire run.
        est_tokens: u64,
        /// Estimated wall-clock seconds at 50 tokens / second.
        est_wall_seconds: u64,
    },

    /// The plan was approved (by the user or automatically when
    /// `hitl_mode == None`).
    PlanApproved,

    /// The plan was rejected by the user.  The orchestrator will either
    /// re-plan or stop, depending on the caller's retry policy.
    PlanRejected {
        /// Optional user-provided rejection reason / edit instructions.
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },

    // ── HITL gates ───────────────────────────────────────────────────────
    /// The orchestrator is paused waiting for human approval.
    ///
    /// The frontend should display the `kind` payload and allow the user
    /// to approve or reject.  The executor resumes when it receives an
    /// `ApprovalResponse` back-channel message.
    AwaitingApproval {
        /// Unique id for this approval request (correlates with the
        /// back-channel `ApprovalResponse`).
        approval_id: String,
        /// What is being approved.
        kind: ApprovalKind,
    },

    // ── node lifecycle ───────────────────────────────────────────────────
    /// A worker node has started executing.
    NodeStarted {
        /// Unique id of the node.
        node_id: String,
        /// The worker's goal text.
        goal: String,
    },

    /// Incremental text token from the currently-executing worker node.
    NodeTextDelta {
        /// Source node id.
        node_id: String,
        /// The new text fragment.
        delta: String,
    },

    /// Incremental reasoning / chain-of-thought token from a worker node
    /// (for models that expose `CoT`).
    NodeReasoningDelta {
        /// Source node id.
        node_id: String,
        /// The reasoning fragment.
        delta: String,
    },

    /// Prompt-processing progress during a worker's LLM pre-fill phase.
    NodeProgress {
        /// Source node id.
        node_id: String,
        /// Tokens processed so far.
        processed: u32,
        /// Total tokens in the prompt.
        total: u32,
        /// Tokens served from the KV cache.
        cached: u32,
        /// Wall-clock time elapsed in milliseconds.
        time_ms: u64,
    },

    /// A worker node has initiated a tool call.
    NodeToolCallStart {
        /// Source node id.
        node_id: String,
        /// The tool call details.
        tool_call: ToolCall,
        /// Human-readable display name for the tool.
        display_name: String,
        /// Optional one-line summary of the arguments for UI rendering.
        #[serde(skip_serializing_if = "Option::is_none")]
        args_summary: Option<String>,
    },

    /// A tool call by a worker node has completed.
    NodeToolCallComplete {
        /// Source node id.
        node_id: String,
        /// The name of the tool that ran.
        tool_name: String,
        /// The tool's result payload.
        result: ToolResult,
        /// Human-readable display name.
        display_name: String,
        /// Human-readable elapsed time (e.g. `"1.2s"`).
        duration_display: String,
    },

    /// A non-fatal warning from a worker node's agent loop.
    NodeSystemWarning {
        /// Source node id.
        node_id: String,
        /// Warning message text.
        message: String,
        /// Optional actionable hint for the user.
        #[serde(skip_serializing_if = "Option::is_none")]
        suggested_action: Option<String>,
    },

    /// A worker node's output is being compacted before downstream
    /// nodes receive it as context.
    NodeCompacting {
        /// Source node id.
        node_id: String,
    },

    /// A worker node finished successfully.
    NodeComplete {
        /// Source node id.
        node_id: String,
        /// First ≤ 200 characters of the output (for UI preview).
        output_preview: String,
    },

    /// A worker node failed with an unrecoverable error.
    ///
    /// The orchestrator will mark all downstream nodes as
    /// [`NodeStatus::Skipped`](super::task_graph::NodeStatus::Skipped) and
    /// then emit [`OrchestratorEvent::OrchestratorError`].
    NodeFailed {
        /// Source node id.
        node_id: String,
        /// Error description.
        error: String,
    },

    // ── synthesis ────────────────────────────────────────────────────────
    /// The synthesis phase has started.
    ///
    /// The synthesiser assembles all node outputs into a unified answer.
    SynthesisStart,

    /// Prompt-processing progress during the synthesis LLM call.
    SynthesisProgress {
        /// Tokens processed so far.
        processed: u32,
        /// Total tokens in the prompt.
        total: u32,
        /// Tokens served from the KV cache.
        cached: u32,
        /// Wall-clock elapsed time in milliseconds.
        time_ms: u64,
    },

    /// Incremental text token from the synthesiser.
    SynthesisTextDelta {
        /// The new text fragment.
        delta: String,
    },

    /// The synthesiser has finished.
    SynthesisComplete {
        /// Full synthesised answer.
        content: String,
    },

    // ── terminal ─────────────────────────────────────────────────────────
    /// The orchestrator run completed successfully.
    ///
    /// `answer` is the synthesiser's final output (same as the `content`
    /// field of the preceding [`OrchestratorEvent::SynthesisComplete`]).
    OrchestratorComplete { answer: String },

    /// The orchestrator run failed with an unrecoverable error.
    OrchestratorError { message: String },

    // ── team (v2) ────────────────────────────────────────────────────────
    /// A [`TaskNodeKind::Team`] node has started executing its nested subgraph.
    ///
    /// Emitted by the executor before it begins scheduling the first wave of
    /// the team's subgraph.  Paired with [`OrchestratorEvent::TeamSynthesized`]
    /// when the subgraph completes.
    TeamStarted {
        /// The id of the `Team` node in the parent graph.
        team_id: String,
        /// The optional specialist role assigned to this team.
        #[serde(skip_serializing_if = "Option::is_none")]
        role: Option<RoleId>,
    },

    /// A [`TaskNodeKind::Team`] node's subgraph has completed and its output
    /// has been compacted for passing to downstream nodes in the parent graph.
    ///
    /// The `compacted_output` is what downstream nodes receive as context from
    /// this team node — identical in shape to a leaf node's `compacted_output`.
    TeamSynthesized {
        /// The id of the `Team` node in the parent graph.
        team_id: String,
        /// Compacted summary of the team's synthesised output.
        compacted_output: String,
    },

    // ── spawn (Phase I) ──────────────────────────────────────────────────
    /// A worker node requested dynamic team spawning and the executor approved
    /// it; the child sub-team subgraph has been planned and is about to run.
    ///
    /// `parent_node_id` is the leaf node that triggered the spawn.  The child
    /// graph runs as a nested [`run_wave_loop`] inside the parent's wave.
    SubteamSpawned {
        /// The leaf node that triggered the spawn.
        parent_node_id: String,
        /// One-line summary of the child graph that was planned.
        child_graph_summary: String,
    },

    // ── steering (Phase K) ───────────────────────────────────────────────
    /// A [`GraphDiff`] was applied to the task graph at a wave boundary.
    ///
    /// Emitted after [`super::task_graph::TaskGraph::apply_diff`] succeeds.
    /// Informational only — execution continues with the updated graph.
    SteeringApplied {
        /// The diff that was applied.
        diff: GraphDiff,
        /// Zero-based wave index at which the diff was applied.
        applied_at_wave: u32,
    },
}
