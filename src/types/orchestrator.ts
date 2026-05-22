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
  | ReplanAttemptEvent
  | OrchestratorCompleteEvent
  | OrchestratorErrorEvent;

export interface PlanProposedEvent {
  type: 'plan_proposed';
  graph: TaskGraph;
}

export interface ReplanAttemptEvent {
  type: 'replan_attempt';
  attempt: number;
  reason: string;
}

export interface OrchestratorCompleteEvent {
  type: 'orchestrator_complete';
  answer: string;
}

export interface OrchestratorErrorEvent {
  type: 'orchestrator_error';
  message: string;
}
