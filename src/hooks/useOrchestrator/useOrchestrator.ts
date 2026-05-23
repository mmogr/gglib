/**
 * Orchestrator hook.
 *
 * Bridges `runOrchestrator` / `resumeOrchestratorRun` SSE events to the
 * `OrchestratorContext` reducer. Manages AbortController lifecycle so the
 * user can cancel mid-stream.
 *
 * @module hooks/useOrchestrator
 */

import { useCallback, useRef } from 'react';
import { useOrchestratorContext, type OrchestratorAction } from '../../contexts/OrchestratorContext';
import {
  runOrchestrator,
  approveOrchestrator,
  listOrchestratorRuns,
  resumeOrchestratorRun,
} from '../../services/clients/orchestrator';
import type { OrchestratorEvent, ApprovalDecisionPayload, OrchestratorRunStatus } from '../../types/orchestrator';
import { appLogger } from '../../services/platform';

export interface UseOrchestratorOptions {
  serverPort: number;
  model?: string;
}

export interface UseOrchestratorReturn {
  session: ReturnType<typeof useOrchestratorContext>['session'];
  run: (goal: string, hitlMode?: string, maxWorkerConcurrency?: number) => Promise<void>;
  resume: (runId: string) => Promise<void>;
  cancel: () => void;
  reset: () => void;
  approve: (payload: ApprovalDecisionPayload) => Promise<void>;
  loadRuns: (status?: OrchestratorRunStatus) => Promise<void>;
  isStreaming: boolean;
}

/** Map a raw SSE OrchestratorEvent to a typed OrchestratorAction. */
function eventToAction(event: OrchestratorEvent): OrchestratorAction | null {
  switch (event.type) {
    case 'plan_proposed':
      return { type: 'PLAN_PROPOSED', graph: event.graph };
    case 'run_cost_estimate':
      return {
        type: 'SET_COST_ESTIMATE',
        nodeCount: event.node_count,
        estTokens: event.est_tokens,
        estWallSeconds: event.est_wall_seconds,
      };
    case 'plan_approved':
      return { type: 'PLAN_APPROVED' };
    case 'plan_rejected':
      return { type: 'PLAN_REJECTED', reason: event.reason };
    case 'replan_attempt':
      return { type: 'REPLAN_ATTEMPT', attempt: event.attempt, reason: event.reason };
    case 'awaiting_approval':
      return { type: 'AWAITING_APPROVAL', approvalId: event.approval_id, kind: event.kind };
    case 'node_started':
      return { type: 'NODE_STARTED', nodeId: event.node_id, goal: event.goal };
    case 'node_text_delta':
      return { type: 'NODE_TEXT_DELTA', nodeId: event.node_id, delta: event.delta };
    case 'node_tool_call_start':
      return {
        type: 'NODE_TOOL_CALL_START',
        nodeId: event.node_id,
        displayName: event.display_name,
        argsSummary: event.args_summary,
      };
    case 'node_tool_call_complete':
      return {
        type: 'NODE_TOOL_CALL_COMPLETE',
        nodeId: event.node_id,
        toolName: event.tool_name,
        displayName: event.display_name,
        durationDisplay: event.duration_display,
      };
    case 'node_compacting':
      return { type: 'NODE_COMPACTING', nodeId: event.node_id };
    case 'node_complete':
      return { type: 'NODE_COMPLETE', nodeId: event.node_id, outputPreview: event.output_preview };
    case 'node_failed':
      return { type: 'NODE_FAILED', nodeId: event.node_id, error: event.error };
    case 'synthesis_start':
      return { type: 'SYNTHESIS_START' };
    case 'synthesis_text_delta':
      return { type: 'SYNTHESIS_TEXT_DELTA', delta: event.delta };
    case 'synthesis_complete':
      return { type: 'SYNTHESIS_COMPLETE', content: event.content };
    case 'orchestrator_complete':
      return { type: 'ORCHESTRATOR_COMPLETE', answer: event.answer };
    case 'orchestrator_error':
      return { type: 'ORCHESTRATOR_ERROR', message: event.message };
    // Informational events — no reducer action needed
    case 'node_reasoning_delta':
    case 'node_progress':
    case 'node_system_warning':
    case 'synthesis_progress':
    case 'team_started':
    case 'team_synthesized':
    case 'subteam_spawned':
      return null;
    case 'steering_applied':
      return { type: 'SET_PENDING_DIFF', diff: event.diff };
    default:
      return null;
  }
}

export function useOrchestrator({ serverPort, model }: UseOrchestratorOptions): UseOrchestratorReturn {
  const { session, dispatch } = useOrchestratorContext();
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
          (event: OrchestratorEvent) => {
            const action = eventToAction(event);
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
        await resumeOrchestratorRun(
          runId,
          serverPort,
          model ?? undefined,
          (event: OrchestratorEvent) => {
            const action = eventToAction(event);
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

  return {
    session,
    run,
    resume,
    cancel,
    reset,
    approve,
    loadRuns,
    isStreaming: streamingRef.current,
  };
}
