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

export interface TaskNode {
  id: string;
  goal: string;
  depends_on: string[];
  tool_allowlist: string[];
  status: string;
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
  | OrchestratorErrorEvent;

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

// ─── HITL / approval types ───────────────────────────────────────────────────

export type ApprovalKind =
  | { kind: 'plan' }
  | { kind: 'node'; node_id: string }
  | { kind: 'tool'; node_id: string; tool_name: string };

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

