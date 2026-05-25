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
import { useCouncilContext, type OrchestratorAction } from '../../contexts/CouncilContext';
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

/** Map a raw SSE CouncilEvent to a typed OrchestratorAction. */
function eventToAction(event: CouncilEvent): OrchestratorAction | null {
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
    case 'wave_completed':
      return null;
    case 'steering_applied':
      return { type: 'SET_PENDING_DIFF', diff: event.diff };
    // Debate events (Phase N)
    case 'debate_round_started':
      return { type: 'DEBATE_ROUND_STARTED', nodeId: event.node_id, round: event.round };
    case 'debate_agent_turn_started':
      return {
        type: 'DEBATE_AGENT_TURN_STARTED',
        nodeId: event.node_id, agentId: event.agent_id,
        agentName: event.agent_name, color: event.color,
        round: event.round, contentiousness: event.contentiousness,
      };
    case 'debate_agent_text_delta':
      return { type: 'DEBATE_AGENT_TEXT_DELTA', nodeId: event.node_id, agentId: event.agent_id, delta: event.delta };
    case 'debate_agent_reasoning_delta':
      return null; // reasoning not rendered in main thread
    case 'debate_agent_tool_call_start':
      return {
        type: 'DEBATE_AGENT_TOOL_CALL_START',
        nodeId: event.node_id, agentId: event.agent_id,
        displayName: event.display_name, argsSummary: event.args_summary,
      };
    case 'debate_agent_tool_call_complete':
      return {
        type: 'DEBATE_AGENT_TOOL_CALL_COMPLETE',
        nodeId: event.node_id, agentId: event.agent_id,
        displayName: event.display_name, durationDisplay: event.duration_display,
      };
    case 'debate_agent_turn_complete':
      return {
        type: 'DEBATE_AGENT_TURN_COMPLETE',
        nodeId: event.node_id, agentId: event.agent_id,
        round: event.round, finalText: event.final_text,
      };
    case 'debate_judge_started':
      return { type: 'DEBATE_JUDGE_STARTED', nodeId: event.node_id, round: event.round };
    case 'debate_judge_text_delta':
      return { type: 'DEBATE_JUDGE_TEXT_DELTA', nodeId: event.node_id, delta: event.delta };
    case 'debate_judge_summary':
      return {
        type: 'DEBATE_JUDGE_SUMMARY',
        nodeId: event.node_id, round: event.round,
        consensusReached: event.consensus_reached,
        earlyStopRecommended: event.early_stop_recommended,
        assessmentText: event.assessment_text,
      };
    case 'debate_round_compacted':
      return { type: 'DEBATE_ROUND_COMPACTED', nodeId: event.node_id, round: event.round, summary: event.summary };
    case 'debate_stance_map':
      return { type: 'DEBATE_STANCE_MAP', nodeId: event.node_id, stances: event.stances };
    case 'debate_synthesis_started':
      return { type: 'DEBATE_SYNTHESIS_STARTED', nodeId: event.node_id };
    case 'debate_synthesis_text_delta':
      return { type: 'DEBATE_SYNTHESIS_TEXT_DELTA', nodeId: event.node_id, delta: event.delta };
    case 'debate_synthesis_complete':
      return { type: 'DEBATE_SYNTHESIS_COMPLETE', nodeId: event.node_id, finalText: event.final_text };
    default:
      return null;
  }
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
        await resumeCouncilRun(
          runId,
          serverPort,
          model ?? undefined,
          (event: CouncilEvent) => {
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
            const action = eventToAction(event);
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
