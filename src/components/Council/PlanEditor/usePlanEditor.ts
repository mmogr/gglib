/**
 * usePlanEditor — undo-stack state manager for the PlanEditor component.
 *
 * Holds a `PlanEditorDraft` (original + current working copy + ordered
 * undo stack) and exposes typed operations for mutating the plan.
 *
 * All ops are applied via the pure `applyPlanEditorOp` function; state
 * is always rebuilt from scratch by reducing the `applied` list over
 * `original` so the undo implementation is trivially correct.
 *
 * @module components/Council/PlanEditor/usePlanEditor
 */

import { useCallback, useState } from 'react';
import type { TaskGraph } from '../../../types/council';
import type {
  PlanEditorDraft,
  PlanEditorOp,
  TrackedDiff,
} from '../../../types/graph-diff';
import {
  applyPlanEditorOp,
  describePlanEditorOp,
  newDiffId,
} from '../../../types/graph-diff';

// ─── Public interface ─────────────────────────────────────────────────────────

export interface UsePlanEditorReturn {
  /** Current draft state. Stable reference when nothing has changed. */
  draft: PlanEditorDraft;
  /**
   * Apply one edit op to the current graph.
   * Silently no-ops on validation failure (node not found, etc.) so
   * callers do not need to guard — the UI simply doesn't update.
   */
  applyOp: (op: PlanEditorOp) => void;
  /**
   * Remove the most-recently-applied op and rebuild `draft.current`
   * by replaying all remaining ops from `original`.
   */
  undo: () => void;
  /** Reset the draft to the original graph, discarding all applied ops. */
  reset: () => void;
  /** True when there is at least one op in the undo stack. */
  canUndo: boolean;
}

// ─── Internal helpers ─────────────────────────────────────────────────────────

/** Rebuild a `PlanEditorDraft` by replaying `applied` over `original`. */
function buildDraft(original: TaskGraph, applied: TrackedDiff[]): PlanEditorDraft {
  const current = applied.reduce<TaskGraph>(
    (g, t) => applyPlanEditorOp(g, t.op),
    original,
  );
  return { original, current, applied, isDirty: applied.length > 0 };
}

// ─── Hook ─────────────────────────────────────────────────────────────────────

/**
 * @param initialGraph  The `TaskGraph` from the `plan_proposed` SSE event.
 *                      Captured once; the hook does NOT react to prop changes.
 */
export function usePlanEditor(initialGraph: TaskGraph): UsePlanEditorReturn {
  const [draft, setDraft] = useState<PlanEditorDraft>(() =>
    buildDraft(initialGraph, []),
  );

  const applyOp = useCallback((op: PlanEditorOp) => {
    setDraft(prev => {
      try {
        const description = describePlanEditorOp(op, prev.current);
        const tracked: TrackedDiff = {
          id: newDiffId(),
          op,
          description,
          timestamp: Date.now(),
        };
        return buildDraft(prev.original, [...prev.applied, tracked]);
      } catch {
        // Validation failed (e.g. node not found) — leave state unchanged.
        return prev;
      }
    });
  }, []);

  const undo = useCallback(() => {
    setDraft(prev => {
      if (prev.applied.length === 0) return prev;
      return buildDraft(prev.original, prev.applied.slice(0, -1));
    });
  }, []);

  const reset = useCallback(() => {
    setDraft(prev => buildDraft(prev.original, []));
  }, []);

  return { draft, applyOp, undo, reset, canUndo: draft.applied.length > 0 };
}
