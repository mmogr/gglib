//! SSE event types emitted during an orchestrator run.
//!
//! [`CouncilEvent`] is the **single source of truth** for the wire format
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
//!   → CouncilComplete
//! ```
//!
//! Error paths emit [`CouncilEvent::NodeFailed`] or
//! [`CouncilEvent::CouncilError`] and then the stream closes.

use serde::{Deserialize, Serialize};

use crate::domain::agent::{ToolCall, ToolResult};
use crate::domain::council::role_catalog::RoleId;

use super::task_graph::{GraphDiff, TaskGraph};

/// Channel capacity for the orchestrator event sender.
///
/// Larger than the per-agent channel to accommodate bursts from multiple
/// concurrent worker nodes plus orchestration bookkeeping events.
pub const COUNCIL_EVENT_CHANNEL_CAPACITY: usize = 8_192;

// =============================================================================
// ApprovalKind
// =============================================================================

/// Describes what the human-in-the-loop gate is waiting for approval on.
///
/// Carried inside [`CouncilEvent::AwaitingApproval`].
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
// AgentStance
// =============================================================================

/// How a debating agent's position changed over the course of a debate.
///
/// Carried inside [`CouncilEvent::DebateStanceMap`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StanceOutcome {
    /// The agent maintained its original position throughout all rounds.
    Held,
    /// The agent moved toward a different position by the final round.
    Shifted,
    /// The agent explicitly conceded to another agent's argument.
    Conceded,
}

/// The final stance outcome for a single debating agent.
///
/// Carried inside [`CouncilEvent::DebateStanceMap`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStance {
    /// Short id of the agent (matches [`DebateAgent::id`]).
    ///
    /// [`DebateAgent::id`]: super::task_graph::DebateAgent::id
    pub agent_id: String,
    /// Whether the agent held, shifted, or conceded.
    pub outcome: StanceOutcome,
}

// =============================================================================
// CouncilEvent
// =============================================================================

/// A single event in an orchestrator execution stream.
///
/// Consumers receive these over SSE (web) or an `mpsc` channel (CLI).
/// Each variant is independently useful — the frontend can render
/// progressively as events arrive without buffering the full stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CouncilEvent {
    // ── planning ─────────────────────────────────────────────────────────
    /// The director has produced an initial task plan.
    ///
    /// If `hitl_mode` requires plan approval, the executor immediately
    /// emits [`CouncilEvent::AwaitingApproval`] after this and
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
    /// [`CouncilEvent::PlanProposed`].
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
    /// then emit [`CouncilEvent::CouncilError`].
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
    /// field of the preceding [`CouncilEvent::SynthesisComplete`]).
    CouncilComplete { answer: String },

    /// The orchestrator run failed with an unrecoverable error.
    CouncilError { message: String },

    // ── team (v2) ────────────────────────────────────────────────────────
    /// A [`TaskNodeKind::Team`] node has started executing its nested subgraph.
    ///
    /// Emitted by the executor before it begins scheduling the first wave of
    /// the team's subgraph.  Paired with [`CouncilEvent::TeamSynthesized`]
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

    // ── debate (Phase N) ─────────────────────────────────────────────────
    /// A new debate round has started inside a [`TaskNodeKind::Debate`] node.
    ///
    /// Emitted once per round, before any agent turn events for that round.
    ///
    /// [`TaskNodeKind::Debate`]: super::task_graph::TaskNodeKind::Debate
    DebateRoundStarted {
        /// The id of the `Debate` node in the parent graph.
        node_id: String,
        /// 1-based round number.
        round: u32,
    },

    /// An agent's turn within a debate round has started.
    ///
    /// Immediately followed by zero or more [`CouncilEvent::DebateAgentTextDelta`]
    /// events for this agent and then [`CouncilEvent::DebateAgentTurnComplete`].
    DebateAgentTurnStarted {
        /// The id of the `Debate` node.
        node_id: String,
        /// Short id of the agent (matches [`DebateAgent::id`]).
        ///
        /// [`DebateAgent::id`]: super::task_graph::DebateAgent::id
        agent_id: String,
        /// Display name of the agent.
        agent_name: String,
        /// Hex colour code (`#rrggbb`) for this agent's text in the UI.
        color: String,
        /// 1-based round number.
        round: u32,
        /// Temperature the LLM was called with (mapped from `contentiousness`).
        contentiousness: f32,
    },

    /// Incremental text token from a debating agent.
    DebateAgentTextDelta {
        /// The id of the `Debate` node.
        node_id: String,
        /// Short id of the agent.
        agent_id: String,
        /// The new text fragment.
        delta: String,
    },

    /// Incremental reasoning / chain-of-thought token from a debating agent.
    DebateAgentReasoningDelta {
        /// The id of the `Debate` node.
        node_id: String,
        /// Short id of the agent.
        agent_id: String,
        /// The reasoning fragment.
        delta: String,
    },

    /// Prompt-processing progress during a debating agent's LLM pre-fill phase.
    DebateAgentProgress {
        /// The id of the `Debate` node.
        node_id: String,
        /// Short id of the agent.
        agent_id: String,
        /// Tokens processed so far.
        processed: u32,
        /// Total tokens in the prompt.
        total: u32,
        /// Tokens served from the KV cache.
        cached: u32,
        /// Wall-clock time elapsed in milliseconds.
        time_ms: u64,
    },

    /// A debating agent has initiated a tool call.
    DebateAgentToolCallStart {
        /// The id of the `Debate` node.
        node_id: String,
        /// Short id of the agent.
        agent_id: String,
        /// The tool call details.
        tool_call: ToolCall,
        /// Human-readable display name for the tool.
        display_name: String,
        /// Optional one-line summary of the arguments for UI rendering.
        #[serde(skip_serializing_if = "Option::is_none")]
        args_summary: Option<String>,
    },

    /// A tool call by a debating agent has completed.
    DebateAgentToolCallComplete {
        /// The id of the `Debate` node.
        node_id: String,
        /// Short id of the agent.
        agent_id: String,
        /// The tool's result payload.
        result: ToolResult,
        /// Human-readable display name.
        display_name: String,
        /// Human-readable elapsed time (e.g. `"1.2s"`).
        duration_display: String,
    },

    /// An agent's turn within a debate round has finished.
    DebateAgentTurnComplete {
        /// The id of the `Debate` node.
        node_id: String,
        /// Short id of the agent.
        agent_id: String,
        /// 1-based round number.
        round: u32,
        /// The agent's complete response for this turn.
        final_text: String,
    },

    /// The judge LLM call for a debate round has started.
    ///
    /// Only emitted when a [`DebateJudgeConfig`] is present.
    ///
    /// [`DebateJudgeConfig`]: super::task_graph::DebateJudgeConfig
    DebateJudgeStarted {
        /// The id of the `Debate` node.
        node_id: String,
        /// 1-based round number being judged.
        round: u32,
    },

    /// Incremental text token from the debate judge.
    DebateJudgeTextDelta {
        /// The id of the `Debate` node.
        node_id: String,
        /// The new text fragment.
        delta: String,
    },

    /// The judge has finished assessing a debate round.
    DebateJudgeSummary {
        /// The id of the `Debate` node.
        node_id: String,
        /// 1-based round number that was judged.
        round: u32,
        /// Whether the judge determined that consensus has been reached.
        consensus_reached: bool,
        /// Whether the judge recommends stopping early (only acted on if
        /// `round >= judge.min_rounds_before_stop`).
        early_stop_recommended: bool,
        /// The judge's full written assessment.
        assessment_text: String,
    },

    /// A debate round's transcript has been compacted to reduce context pressure.
    ///
    /// Only emitted when the running context window would otherwise overflow.
    DebateRoundCompacted {
        /// The id of the `Debate` node.
        node_id: String,
        /// 1-based round number that was compacted.
        round: u32,
        /// Compressed summary replacing the full round transcript.
        summary: String,
    },

    /// Final stance outcomes for all agents after all rounds complete.
    ///
    /// Emitted once after the last debate round, before synthesis starts.
    DebateStanceMap {
        /// The id of the `Debate` node.
        node_id: String,
        /// Per-agent stance outcomes.
        stances: Vec<AgentStance>,
    },

    /// The debate synthesis phase has started.
    ///
    /// The synthesiser assembles the full round history into a verdict.
    DebateSynthesisStarted {
        /// The id of the `Debate` node.
        node_id: String,
    },

    /// Incremental text token from the debate synthesiser.
    DebateSynthesisTextDelta {
        /// The id of the `Debate` node.
        node_id: String,
        /// The new text fragment.
        delta: String,
    },

    /// The debate synthesis has finished.
    ///
    /// `final_text` becomes the node's `output` and is passed to the
    /// compaction step before downstream nodes receive it as context.
    DebateSynthesisComplete {
        /// The id of the `Debate` node.
        node_id: String,
        /// The synthesiser's complete verdict text.
        final_text: String,
    },

    // ── rewind (Phase M) ─────────────────────────────────────────────────
    /// All nodes in a topological wave have completed.
    ///
    /// Emitted once at the end of each wave (depth 0 only).  The frontend
    /// uses these events as scrubber waypoints so the user can rewind to any
    /// completed wave.
    WaveCompleted {
        /// Zero-based index of the wave that just finished.
        wave_index: u32,
        /// Number of nodes that completed in this wave.
        node_count: usize,
    },
}
