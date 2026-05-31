/**
 * useCouncilRunStream — SSE streaming wired into the CouncilRegistry.
 *
 * A registry-scoped counterpart to the legacy `useCouncil` hook.
 * Instead of pulling dispatch from the page-singleton CouncilContext,
 * it accepts `dispatch`, `setAbortController`, and `abort` as injected
 * dependencies from `useCouncilRun(runId)` — giving each run its own
 * isolated stream without any coupling to the singleton.
 *
 * Key differences from `useCouncil`:
 *   - No React context import — purely functional, takes callbacks.
 *   - No `run list` management (that belongs at registry level).
 *   - `startRun` / `cancelRun` / `approve` are the only exported operations
 *     (rewind stays on the full orchestrator page for Phase 1).
 *
 * `eventToAction` is duplicated here rather than re-exported from
 * `useCouncil` so that hook remains untouched (Phase 1 additive
 * constraint). It will be deduplicated in Phase 7 during the final cleanup.
 *
 * @module components/Council/Thread/useCouncilRunStream
 */

import { useCallback } from 'react';
import {
  runOrchestrator,
  approveOrchestrator,
} from '../../../services/clients/council';
import type { OrchestratorAction, CouncilSession } from '../../../contexts/CouncilContext';
import { councilEventToAction } from '../../../utils/councilEventToAction';
import type { CouncilEvent, ApprovalDecisionPayload } from '../../../types/council';
import { appLogger } from '../../../services/platform';

// ─── Hook options / return ────────────────────────────────────────────────────

export interface UseOrchestratorRunStreamOptions {
  /** Dispatch bound to this run's registry slot (from useCouncilRun). */
  dispatch: (action: OrchestratorAction) => void;
  /** Attach an AbortController for the live SSE stream. */
  setAbortController: (ctrl: AbortController) => void;
  /** Abort the current stream (user-initiated cancel). */
  abort: () => void;
  /**
   * Read the current pendingApproval from the run's session.
   * A getter (not a value) so the `approve` callback captures the latest
   * state without needing to be re-created on every session update.
   */
  getPendingApproval: () => CouncilSession['pendingApproval'];
  /** llama.cpp HTTP base port. */
  serverPort: number;
  /** Optional model name override. */
  model?: string;
}

export interface UseOrchestratorRunStreamReturn {
  /**
   * Kick off a new run: dispatches START_RUN, opens the SSE stream, and fans
   * all received events into the registry via `dispatch`.
   */
  startRun: (goal: string, hitlMode?: string) => Promise<void>;
  /** Abort the active SSE stream. */
  cancelRun: () => void;
  /**
   * Submit an approval decision for the current HITL gate.
   * No-op if there is no pending approval.
   */
  approve: (payload: ApprovalDecisionPayload) => Promise<void>;
}

// ─── Hook ─────────────────────────────────────────────────────────────────────

/**
 * Manages the live SSE lifecycle for a single orchestrator run.
 *
 * All state updates flow through the injected `dispatch`, which routes them
 * into the registry so `useCouncilRun(runId)` subscribers receive them.
 *
 * Mount/unmount cleanup (aborting the stream) is the caller's responsibility
 * because this hook does not know its runId — the caller (CouncilThread)
 * handles `unregister` + `abort` in its own cleanup effect.
 */
export function useCouncilRunStream({
  dispatch,
  setAbortController,
  abort,
  getPendingApproval,
  serverPort,
  model,
}: UseOrchestratorRunStreamOptions): UseOrchestratorRunStreamReturn {
  const startRun = useCallback(
    async (goal: string, hitlMode?: string) => {
      // Cancel any existing stream first.
      abort();

      const ctrl = new AbortController();
      setAbortController(ctrl);

      dispatch({ type: 'START_RUN', goal });

      try {
        await runOrchestrator(
          {
            goal,
            port: serverPort,
            model: model ?? undefined,
            hitl_mode: hitlMode && hitlMode !== 'none' ? hitlMode : undefined,
          },
          (event: CouncilEvent) => {
            const action = councilEventToAction(event);
            if (action) dispatch(action);
          },
          ctrl.signal,
        );
      } catch (err: unknown) {
        if (err instanceof Error && err.name !== 'AbortError') {
          dispatch({ type: 'ORCHESTRATOR_ERROR', message: err.message });
          appLogger.error('hook', 'CouncilThread run failed', { error: err.message });
        }
      }
    },
    [abort, setAbortController, dispatch, serverPort, model],
  );

  const cancelRun = useCallback(() => {
    abort();
  }, [abort]);

  const approve = useCallback(
    async (payload: ApprovalDecisionPayload) => {
      const pending = getPendingApproval();
      if (!pending) return;
      const { approvalId } = pending;

      dispatch({ type: 'APPROVAL_SUBMITTING' });
      try {
        await approveOrchestrator(approvalId, payload);
        dispatch({ type: 'APPROVAL_DONE' });
      } catch (err: unknown) {
        const message = err instanceof Error ? err.message : 'Approval failed';
        dispatch({ type: 'ORCHESTRATOR_ERROR', message });
        appLogger.error('hook', 'CouncilThread approval failed', { error: message });
      }
    },
    [getPendingApproval, dispatch],
  );

  return { startRun, cancelRun, approve };
}
