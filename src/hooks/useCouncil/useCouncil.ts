/**
 * Orchestrator hook.
 *
 * Bridges `runOrchestrator` / `resumeCouncilRun` SSE events to the
 * `CouncilContext` reducer. Manages AbortController lifecycle so the
 * user can cancel mid-stream.
 *
 * @module hooks/useCouncil
 */

import { useCallback, useRef } from 'react';
import { useCouncilContext } from '../../contexts/CouncilContext';
import { councilEventToAction } from '../../utils/councilEventToAction';
import {
  runOrchestrator,
  approveOrchestrator,
  listOrchestratorRuns,
  resumeCouncilRun,
  rewindCouncilRun,
} from '../../services/clients/council';
import type { CouncilEvent, ApprovalDecisionPayload, OrchestratorRunStatus } from '../../types/council';
import { appLogger } from '../../services/platform';

export interface UseOrchestratorOptions {
  serverPort: number;
  model?: string;
}

export interface UseOrchestratorReturn {
  session: ReturnType<typeof useCouncilContext>['session'];
  run: (goal: string, hitlMode?: string, maxWorkerConcurrency?: number) => Promise<void>;
  resume: (runId: string) => Promise<void>;
  rewind: (runId: string, waveIndex: number, steeringNote?: string) => Promise<void>;
  cancel: () => void;
  reset: () => void;
  approve: (payload: ApprovalDecisionPayload) => Promise<void>;
  loadRuns: (status?: OrchestratorRunStatus) => Promise<void>;
  isStreaming: boolean;
}

export function useCouncil({ serverPort, model }: UseOrchestratorOptions): UseOrchestratorReturn {
  const { session, dispatch } = useCouncilContext();
  const abortRef = useRef<AbortController | null>(null);
  const streamingRef = useRef(false);

  const cancel = useCallback(() => {
    abortRef.current?.abort();
    abortRef.current = null;
    streamingRef.current = false;
  }, []);

  const reset = useCallback(() => {
    cancel();
    dispatch({ type: 'RESET' });
  }, [cancel, dispatch]);

  const run = useCallback(
    async (goal: string, hitlMode?: string, maxWorkerConcurrency?: number) => {
      cancel();
      const ctrl = new AbortController();
      abortRef.current = ctrl;
      streamingRef.current = true;

      dispatch({ type: 'START_RUN', goal });

      try {
        await runOrchestrator(
          {
            goal,
            port: serverPort,
            model: model ?? undefined,
            hitl_mode: hitlMode && hitlMode !== 'none' ? hitlMode : undefined,
            max_worker_concurrency: maxWorkerConcurrency,
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
          appLogger.error('hook', 'Orchestrator run failed', { error: err.message });
        }
      } finally {
        streamingRef.current = false;
      }
    },
    [cancel, dispatch, serverPort, model],
  );

  const resume = useCallback(
    async (runId: string) => {
      cancel();
      const ctrl = new AbortController();
      abortRef.current = ctrl;
      streamingRef.current = true;

      dispatch({ type: 'START_RUN', goal: '' });

      try {
        await resumeCouncilRun(
          runId,
          serverPort,
          model ?? undefined,
          (event: CouncilEvent) => {
            const action = councilEventToAction(event);
            if (action) dispatch(action);
          },
          ctrl.signal,
        );
      } catch (err: unknown) {
        if (err instanceof Error && err.name !== 'AbortError') {
          dispatch({ type: 'ORCHESTRATOR_ERROR', message: err.message });
          appLogger.error('hook', 'Orchestrator resume failed', { error: err.message });
        }
      } finally {
        streamingRef.current = false;
      }
    },
    [cancel, dispatch, serverPort, model],
  );

  const approve = useCallback(
    async (payload: ApprovalDecisionPayload) => {
      if (!session.pendingApproval) return;
      const { approvalId } = session.pendingApproval;

      dispatch({ type: 'APPROVAL_SUBMITTING' });
      try {
        await approveOrchestrator(approvalId, payload);
        dispatch({ type: 'APPROVAL_DONE' });
      } catch (err: unknown) {
        const message = err instanceof Error ? err.message : 'Approval failed';
        dispatch({ type: 'ORCHESTRATOR_ERROR', message });
        appLogger.error('hook', 'Orchestrator approval failed', { error: message });
      }
    },
    [session.pendingApproval, dispatch],
  );

  const loadRuns = useCallback(
    async (status?: OrchestratorRunStatus) => {
      dispatch({ type: 'RUNS_LOADING' });
      try {
        const runs = await listOrchestratorRuns(status);
        dispatch({ type: 'RUNS_LOADED', runs });
      } catch (err: unknown) {
        const message = err instanceof Error ? err.message : 'Failed to load runs';
        appLogger.error('hook', 'Failed to load orchestrator runs', { error: message });
        dispatch({ type: 'RUNS_ERROR' });
      }
    },
    [dispatch],
  );

  const rewind = useCallback(
    async (runId: string, waveIndex: number, steeringNote?: string) => {
      cancel();
      const ctrl = new AbortController();
      abortRef.current = ctrl;
      streamingRef.current = true;

      dispatch({ type: 'START_RUN', goal: '' });

      try {
        await rewindCouncilRun(
          runId,
          { port: serverPort, model: model ?? undefined, wave_index: waveIndex, steering_note: steeringNote },
          (event: CouncilEvent) => {
            const action = councilEventToAction(event);
            if (action) dispatch(action);
          },
          ctrl.signal,
        );
      } catch (err: unknown) {
        if (err instanceof Error && err.name !== 'AbortError') {
          dispatch({ type: 'ORCHESTRATOR_ERROR', message: err.message });
          appLogger.error('hook', 'Orchestrator rewind failed', { error: err.message });
        }
      } finally {
        streamingRef.current = false;
      }
    },
    [cancel, dispatch, serverPort, model],
  );

  return {
    session,
    run,
    resume,
    rewind,
    cancel,
    reset,
    approve,
    loadRuns,
    isStreaming: streamingRef.current,
  };
}
