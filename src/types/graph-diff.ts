/**
 * PlanEditor — frontend-only diff tracking types.
 *
 * These types are FRONTEND-ONLY.  When the user approves with edits the
 * consumer serialises `draft.current` as the full `edited_graph` in an
 * `ApprovalDecisionPayload` — the individual ops never travel to the
 * backend.  They exist solely for undo-stack management and the
 * DiffChangelog display.
 *
 * Some ops (e.g. `set_goal`, `set_deps`) do not exist in the Rust
 * `GraphDiff` enum.  That is intentional: the backend receives the
 * finished graph, not individual diffs.
 *
 * @module types/graph-diff
 */

import type { TaskGraph, TaskNode } from './council';

// Re-export the backend-aligned union for convenience.
export type { GraphDiff } from './council';

// ─── Named op interfaces ──────────────────────────────────────────────────────
// Individual arms of GraphDiff extracted for use in typed helper functions
// and test assertions.

export interface AddNodeOp {
  op: 'add_node';
  node: TaskNode;
}

export interface RemoveNodeOp {
  op: 'remove_node';
  id: string;
}

export interface SplitNodeOp {
  op: 'split_node';
  id: string;
  into: TaskNode[];
}

export interface RerouteEdgeOp {
  op: 'reroute_edge';
  node_id: string;
  old_dep: string;
  new_dep: string;
}

export interface SetRoleOp {
  op: 'set_role';
  id: string;
  role: string | null;
}

export interface SetToolsOp {
  op: 'set_tools';
  id: string;
  tool_allowlist: string[];
}

export interface WrapInTeamOp {
  op: 'wrap_in_team';
  ids: string[];
  team_id: string;
  team_goal: string;
}

// ─── Frontend-only ops ────────────────────────────────────────────────────────
// Not in the Rust GraphDiff enum.  Applied to `draft.current` but serialised
// as a full graph update (not as diffs) when sending to the backend.

/** Edit a node's natural language goal text. */
export interface SetGoalOp {
  op: 'set_goal';
  id: string;
  goal: string;
}

/** Replace the full `depends_on` list for a node. */
export interface SetDepsOp {
  op: 'set_deps';
  id: string;
  depends_on: string[];
}

// ─── PlanEditorOp (superset of GraphDiff) ────────────────────────────────────

/**
 * Every edit the PlanEditor can apply.
 * A strict superset of the backend `GraphDiff` union; includes
 * frontend-only ops.
 */
export type PlanEditorOp =
  | AddNodeOp
  | RemoveNodeOp
  | SplitNodeOp
  | RerouteEdgeOp
  | SetRoleOp
  | SetToolsOp
  | WrapInTeamOp
  | SetGoalOp
  | SetDepsOp;

// ─── Diff validation ──────────────────────────────────────────────────────────

export interface DiffValidationError {
  op: PlanEditorOp['op'];
  message: string;
  /** ID of the affected node, if applicable. */
  nodeId?: string;
}

// ─── TrackedDiff ──────────────────────────────────────────────────────────────

/**
 * A single applied edit with provenance metadata.
 * Stored in `PlanEditorDraft.applied` for undo support and the
 * DiffChangelog UI.
 */
export interface TrackedDiff {
  /**
   * Stable local ID.
   * Uses `crypto.randomUUID()` where available, otherwise a Date.now
   * base-36 + Math.random suffix.
   */
  id: string;
  op: PlanEditorOp;
  /** Human-readable one-liner shown in the DiffChangelog. */
  description: string;
  /** `Date.now()` at the time the op was applied. */
  timestamp: number;
}

// ─── PlanEditorDraft ──────────────────────────────────────────────────────────

/**
 * The working state managed by `usePlanEditor`.
 *
 * `original` is never mutated.  Every `applyOp` call produces a new
 * `current` via structural sharing (spread + clone) — no in-place
 * mutation.
 */
export interface PlanEditorDraft {
  /** Original graph from `plan_proposed` — immutable reference baseline. */
  original: TaskGraph;
  /** Working copy with every pending diff applied in order. */
  current: TaskGraph;
  /** Ordered list of applied diffs — the undo stack. */
  applied: TrackedDiff[];
  /** True when `current` differs from `original`. */
  isDirty: boolean;
}

// ─── Pure apply function ──────────────────────────────────────────────────────

/**
 * Apply a single `PlanEditorOp` to `graph` and return a new `TaskGraph`.
 *
 * Never mutates `graph`.  Throws a `DiffValidationError`-shaped object
 * when the op references a node that does not exist (except `add_node`
 * which creates it).
 */
export function applyPlanEditorOp(graph: TaskGraph, op: PlanEditorOp): TaskGraph {
  switch (op.op) {
    case 'add_node': {
      return { ...graph, nodes: { ...graph.nodes, [op.node.id]: op.node } };
    }

    case 'remove_node': {
      const { [op.id]: _removed, ...rest } = graph.nodes;
      // Clean up stale depends_on references in surviving nodes.
      const cleaned: TaskGraph['nodes'] = Object.fromEntries(
        Object.entries(rest).map(([k, n]) => [
          k,
          { ...n, depends_on: n.depends_on.filter(d => d !== op.id) },
        ]),
      );
      return { ...graph, nodes: cleaned };
    }

    case 'split_node': {
      const { [op.id]: _orig, ...rest } = graph.nodes;
      const additions = Object.fromEntries(op.into.map(n => [n.id, n]));
      return { ...graph, nodes: { ...rest, ...additions } };
    }

    case 'reroute_edge': {
      const node = graph.nodes[op.node_id];
      if (!node) {
        throw { op: op.op, message: `Node "${op.node_id}" not found`, nodeId: op.node_id } satisfies DiffValidationError;
      }
      return {
        ...graph,
        nodes: {
          ...graph.nodes,
          [op.node_id]: {
            ...node,
            depends_on: node.depends_on.map(d => (d === op.old_dep ? op.new_dep : d)),
          },
        },
      };
    }

    case 'set_role': {
      const node = graph.nodes[op.id];
      if (!node) {
        throw { op: op.op, message: `Node "${op.id}" not found`, nodeId: op.id } satisfies DiffValidationError;
      }
      return { ...graph, nodes: { ...graph.nodes, [op.id]: { ...node, role: op.role } } };
    }

    case 'set_tools': {
      const node = graph.nodes[op.id];
      if (!node) {
        throw { op: op.op, message: `Node "${op.id}" not found`, nodeId: op.id } satisfies DiffValidationError;
      }
      return {
        ...graph,
        nodes: { ...graph.nodes, [op.id]: { ...node, tool_allowlist: op.tool_allowlist } },
      };
    }

    case 'wrap_in_team': {
      const teamNodes = Object.fromEntries(
        op.ids.flatMap(id => (graph.nodes[id] ? [[id, graph.nodes[id]]] : [])),
      );
      const remaining = Object.fromEntries(
        Object.entries(graph.nodes).filter(([k]) => !op.ids.includes(k)),
      );
      const teamNode: TaskNode = {
        id: op.team_id,
        goal: op.team_goal,
        depends_on: [],
        tool_allowlist: [],
        status: 'pending',
        kind: {
          team: {
            subgraph: {
              goal: op.team_goal,
              hitl_mode: graph.hitl_mode,
              nodes: teamNodes,
            },
          },
        },
      };
      return { ...graph, nodes: { ...remaining, [op.team_id]: teamNode } };
    }

    case 'set_goal': {
      const node = graph.nodes[op.id];
      if (!node) {
        throw { op: op.op, message: `Node "${op.id}" not found`, nodeId: op.id } satisfies DiffValidationError;
      }
      return { ...graph, nodes: { ...graph.nodes, [op.id]: { ...node, goal: op.goal } } };
    }

    case 'set_deps': {
      const node = graph.nodes[op.id];
      if (!node) {
        throw { op: op.op, message: `Node "${op.id}" not found`, nodeId: op.id } satisfies DiffValidationError;
      }
      return {
        ...graph,
        nodes: { ...graph.nodes, [op.id]: { ...node, depends_on: op.depends_on } },
      };
    }
  }
}

// ─── Utility helpers ──────────────────────────────────────────────────────────

/** Generate a human-readable one-liner for the DiffChangelog. */
export function describePlanEditorOp(op: PlanEditorOp, graph: TaskGraph): string {
  const label = (id: string) => graph.nodes[id]?.goal.slice(0, 40) ?? id;
  switch (op.op) {
    case 'add_node':
      return `Add node "${op.node.goal.slice(0, 40)}"`;
    case 'remove_node':
      return `Remove "${label(op.id)}"`;
    case 'split_node':
      return `Split "${label(op.id)}" into ${op.into.length} nodes`;
    case 'reroute_edge':
      return `Reroute ${op.node_id}: ${op.old_dep} → ${op.new_dep}`;
    case 'set_role':
      return op.role
        ? `Set role "${op.role}" on "${label(op.id)}"`
        : `Clear role on "${label(op.id)}"`;
    case 'set_tools':
      return `Set tools on "${label(op.id)}" (${op.tool_allowlist.length} allowed)`;
    case 'wrap_in_team':
      return `Wrap ${op.ids.length} nodes into team "${op.team_goal.slice(0, 30)}"`;
    case 'set_goal':
      return `Update goal of "${label(op.id)}"`;
    case 'set_deps':
      return `Update deps of "${label(op.id)}"`;
  }
}

/** Generate a stable local ID for a `TrackedDiff`. */
export function newDiffId(): string {
  return typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function'
    ? crypto.randomUUID()
    : `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
}
