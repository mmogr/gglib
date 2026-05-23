/**
 * Orchestrator — frontend domain types.
 *
 * Mirrors the Rust `orchestrator::events::OrchestratorEvent` discriminated
 * union and the `task_graph::{TaskGraph, TaskNode, HitlMode}` types.
 *
 * Serde configuration on the Rust side:
 *   - `#[serde(tag = "type", rename_all = "snake_case")]` on OrchestratorEvent
 *   - `#[serde(rename_all = "snake_case")]` on HitlMode / NodeStatus
 *
 * @module types/orchestrator
 */

// ─── Task graph domain types ─────────────────────────────────────────────────

export type HitlMode = 'none' | 'approve_plan' | 'approve_each_node' | 'approve_tools';

/**
 * Advisory node-count budget.  Mirrors `task_graph::NodeBudget` (Rust).
 *
 * The `kind` field is produced by `#[serde(tag = "kind", rename_all =
 * "snake_case")]`.
 */
export type NodeBudget =
  | { kind: 'solo' }
  | { kind: 'small_team' }
  | { kind: 'task_force' }
  | { kind: 'department' }
  | { kind: 'custom'; value: number };

/**
 * Mirrors `task_graph::TaskNodeKind` (Rust).
 *
 * - `"leaf"` — a standard single-worker node (default for v1 plans).
 * - `{ team: { subgraph } }` — a compound node that encapsulates a nested
 *   TaskGraph executed as a sub-team.
 */
export type TaskNodeKind = 'leaf' | { team: { subgraph: TaskGraph } };

export interface TaskNode {
  id: string;
  goal: string;
  depends_on: string[];
  tool_allowlist: string[];
  status: string;
  /** Node kind — absent/`"leaf"` for all Phase A–F plans. */
  kind?: TaskNodeKind | null;
  /** Specialist role id, e.g. `"researcher"` or `"critic"`. */
  role?: string | null;
  output?: string | null;
  error?: string | null;
}

export interface TaskGraph {
  goal: string;
  hitl_mode: HitlMode;
  /** Map from node id to TaskNode */
  nodes: Record<string, TaskNode>;
}

// ─── SSE Event discriminated union ──────────────────────────────────────────

/**
 * Mirrors `orchestrator::events::OrchestratorEvent` (Rust).
 *
 * The `type` field is produced by `#[serde(tag = "type", rename_all =
 * "snake_case")]`, so every JSON event carries e.g. `"type":
 * "plan_proposed"`.
 */
export type OrchestratorEvent =
  | PlanProposedEvent
  | PlanApprovedEvent
  | PlanRejectedEvent
  | ReplanAttemptEvent
  | RunCostEstimateEvent
  | SteeringAppliedEvent
  | AwaitingApprovalEvent
  | NodeStartedEvent
  | NodeTextDeltaEvent
  | NodeReasoningDeltaEvent
  | NodeProgressEvent
  | NodeToolCallStartEvent
  | NodeToolCallCompleteEvent
  | NodeSystemWarningEvent
  | NodeCompactingEvent
  | NodeCompleteEvent
  | NodeFailedEvent
  | SynthesisStartEvent
  | SynthesisProgressEvent
  | SynthesisTextDeltaEvent
  | SynthesisCompleteEvent
  | OrchestratorCompleteEvent
  | OrchestratorErrorEvent
  | TeamStartedEvent
  | TeamSynthesizedEvent
  | SubteamSpawnedEvent;

// ─── Planning events ─────────────────────────────────────────────────────────

export interface PlanProposedEvent {
  type: 'plan_proposed';
  graph: TaskGraph;
}

export interface PlanApprovedEvent {
  type: 'plan_approved';
}

export interface PlanRejectedEvent {
  type: 'plan_rejected';
  reason?: string | null;
}

export interface ReplanAttemptEvent {
  type: 'replan_attempt';
  attempt: number;
  reason: string;
}

// ─── Cost estimate event ─────────────────────────────────────────────────────

/**
 * Warn-only cost estimate emitted immediately after `plan_proposed`.
 *
 * Mirrors `orchestrator::events::OrchestratorEvent::RunCostEstimate`.
 */
export interface RunCostEstimateEvent {
  type: 'run_cost_estimate';
  /** Total aggregate node count across all subgraphs. */
  node_count: number;
  /** Rough token estimate (input + output) for the entire run. */
  est_tokens: number;
  /** Estimated wall-clock seconds at 50 tokens/second. */
  est_wall_seconds: number;
}

// ─── Node lifecycle events ───────────────────────────────────────────────────

export interface NodeStartedEvent {
  type: 'node_started';
  node_id: string;
  goal: string;
}

export interface NodeTextDeltaEvent {
  type: 'node_text_delta';
  node_id: string;
  delta: string;
}

export interface NodeReasoningDeltaEvent {
  type: 'node_reasoning_delta';
  node_id: string;
  delta: string;
}

export interface NodeProgressEvent {
  type: 'node_progress';
  node_id: string;
  processed: number;
  total: number;
  cached: number;
  time_ms: number;
}

export interface NodeToolCallStartEvent {
  type: 'node_tool_call_start';
  node_id: string;
  display_name: string;
  args_summary: string;
}

export interface NodeToolCallCompleteEvent {
  type: 'node_tool_call_complete';
  node_id: string;
  tool_name: string;
  display_name: string;
  duration_display: string;
  result: unknown;
}

export interface NodeSystemWarningEvent {
  type: 'node_system_warning';
  node_id: string;
  message: string;
  suggested_action?: string | null;
}

export interface NodeCompactingEvent {
  type: 'node_compacting';
  node_id: string;
}

export interface NodeCompleteEvent {
  type: 'node_complete';
  node_id: string;
  output_preview: string;
}

export interface NodeFailedEvent {
  type: 'node_failed';
  node_id: string;
  error: string;
}

// ─── Synthesis events ─────────────────────────────────────────────────────────

export interface SynthesisStartEvent {
  type: 'synthesis_start';
}

export interface SynthesisProgressEvent {
  type: 'synthesis_progress';
  processed: number;
  total: number;
  cached: number;
  time_ms: number;
}

export interface SynthesisTextDeltaEvent {
  type: 'synthesis_text_delta';
  delta: string;
}

export interface SynthesisCompleteEvent {
  type: 'synthesis_complete';
  content: string;
}

// ─── Terminal events ──────────────────────────────────────────────────────────

export interface OrchestratorCompleteEvent {
  type: 'orchestrator_complete';
  answer: string;
}

export interface OrchestratorErrorEvent {
  type: 'orchestrator_error';
  message: string;
}

// ─── Team events (Phase G / Phase I) ─────────────────────────────────────────

export interface TeamStartedEvent {
  type: 'team_started';
  team_id: string;
  role?: string | null;
}

export interface TeamSynthesizedEvent {
  type: 'team_synthesized';
  team_id: string;
  compacted_output: string;
}

export interface SubteamSpawnedEvent {
  type: 'subteam_spawned';
  parent_node_id: string;
  child_graph_summary: string;
}

// ─── GraphDiff (Phase K) ─────────────────────────────────────────────────────

/**
 * Mirrors `task_graph::GraphDiff` (Rust).
 *
 * Produced by `#[serde(tag = "op", rename_all = "snake_case")]`.
 */
export type GraphDiff =
  | { op: 'add_node'; node: TaskNode }
  | { op: 'remove_node'; id: string }
  | { op: 'split_node'; id: string; into: TaskNode[] }
  | { op: 'reroute_edge'; node_id: string; old_dep: string; new_dep: string }
  | { op: 'set_role'; id: string; role: string | null }
  | { op: 'set_tools'; id: string; tool_allowlist: string[] }
  | { op: 'wrap_in_team'; ids: string[]; team_id: string; team_goal: string };

export interface SteeringAppliedEvent {
  type: 'steering_applied';
  diff: GraphDiff;
  applied_at_wave: number;
}

// ─── HITL / approval types ───────────────────────────────────────────────────

export type ApprovalKind =
  | { kind: 'plan' }
  | { kind: 'node'; node_id: string }
  | { kind: 'tool'; node_id: string; tool_name: string }
  | { kind: 'spawn_subteam'; node_id: string; suggested_roles: string[] };

export interface AwaitingApprovalEvent {
  type: 'awaiting_approval';
  approval_id: string;
  kind: ApprovalKind;
}

export type ApprovalDecisionPayload =
  | { decision: 'approve' }
  | { decision: 'approve_with_edits'; edited_graph: TaskGraph }
  | { decision: 'reject'; reason?: string };

// ─── Run persistence types ───────────────────────────────────────────────────

export type OrchestratorRunStatus =
  | 'running'
  | 'awaiting_approval'
  | 'interrupted'
  | 'completed'
  | 'failed';

export interface OrchestratorRun {
  id: string;
  goal: string;
  graph_json?: string | null;
  status: OrchestratorRunStatus;
  hitl_mode: HitlMode;
  conversation_id?: number | null;
  created_at: string;
  updated_at: string;
}

export interface OrchestratorRunEvent {
  run_id: string;
  seq: number;
  event_json: string;
  created_at: string;
}

