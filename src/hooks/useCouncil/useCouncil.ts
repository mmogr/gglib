/**
 * Council orchestration hook.
 *
 * Bridges the `runCouncil` / `suggestCouncil` client functions to the
 * `CouncilContext` reducer. Manages AbortController lifecycle so the
 * user can cancel a deliberation mid-stream.
 *
 * All UI rendering logic belongs in components — this hook is strictly
 * state orchestration + SSE event dispatch.
 *
 * @module hooks/useCouncil
 */

import { useCallback, useRef } from 'react';
import { useCouncilContext, type CouncilAction } from '../../contexts/CouncilContext';
import { suggestCouncil, runCouncil } from '../../services/clients/council';
import type { CouncilConfig, CouncilEvent, CouncilAgent } from '../../types/council';
import { appLogger } from '../../services/platform';

export interface UseCouncilOptions {
  serverPort: number;
  model?: string;
}

export interface UseCouncilReturn {
  /** Current session state (from context). */
  session: ReturnType<typeof useCouncilContext>['session'];
  /** Request the LLM to design a council for a topic. */
  suggest: (topic: string, agentCount?: number) => Promise<CouncilAgent[] | null>;
  /** Refine the current suggestion with a follow-up instruction. */
  refine: (instruction: string) => Promise<CouncilAgent[] | null>;
  /** Start a deliberation with the given config. */
  run: (config: CouncilConfig) => Promise<void>;
  /** Abort the current deliberation stream. */
  cancel: () => void;
  /** Reset session to idle. */
  reset: () => void;
  /** Whether a deliberation is currently streaming. */
  isStreaming: boolean;
}

/** Map a raw SSE JSON object to a typed CouncilAction dispatch. */
function eventToAction(event: CouncilEvent): CouncilAction | null {
  switch (event.type) {
    case 'agent_turn_start':
      return {
        type: 'AGENT_TURN_START',
        agentId: event.agent_id,
        agentName: event.agent_name,
        color: event.color,
        round: event.round,
        contentiousness: event.contentiousness,
      };
    case 'agent_text_delta':
      return { type: 'AGENT_TEXT_DELTA', agentId: event.agent_id, delta: event.delta };
    case 'agent_reasoning_delta':
      return { type: 'AGENT_REASONING_DELTA', agentId: event.agent_id, delta: event.delta };
    case 'agent_tool_call_start':
      return {
        type: 'AGENT_TOOL_CALL_START',
        toolCall: {
          agentId: event.agent_id,
          toolName: event.tool_call.name,
          displayName: event.display_name,
          argsSummary: event.args_summary,
        },
      };
    case 'agent_tool_call_complete':
      return {
        type: 'AGENT_TOOL_CALL_COMPLETE',
        agentId: event.agent_id,
        toolName: event.tool_name,
        result: { content: event.result.content, isError: event.result.is_error },
        displayName: event.display_name,
        durationDisplay: event.duration_display,
      };
    case 'agent_turn_complete':
      return {
        type: 'AGENT_TURN_COMPLETE',
        contribution: {
          agentId: event.agent_id,
          agentName: '',        // Filled from context by reducer if needed
          color: '',            // Filled from the turn-start event's state
          contentiousness: 0,
          content: event.content,
          coreClaim: event.core_claim,
          round: event.round,
        },
      };
    case 'round_separator':
      return { type: 'ROUND_SEPARATOR', round: event.round };
    case 'synthesis_start':
      return { type: 'SYNTHESIS_START' };
    case 'synthesis_text_delta':
      return { type: 'SYNTHESIS_TEXT_DELTA', delta: event.delta };
    case 'synthesis_complete':
      return { type: 'SYNTHESIS_COMPLETE', content: event.content };
    case 'council_error':
      return { type: 'COUNCIL_ERROR', error: event.message };
    case 'council_complete':
      return { type: 'COUNCIL_COMPLETE' };
    default:
      return null;
  }
}

export function useCouncil({ serverPort, model }: UseCouncilOptions): UseCouncilReturn {
  const { session, dispatch } = useCouncilContext();
  const abortRef = useRef<AbortController | null>(null);

  const cancel = useCallback(() => {
    abortRef.current?.abort();
    abortRef.current = null;
  }, []);

  const reset = useCallback(() => {
    cancel();
    dispatch({ type: 'RESET' });
  }, [cancel, dispatch]);

  const suggest = useCallback(async (topic: string, agentCount = 3): Promise<CouncilAgent[] | null> => {
    dispatch({ type: 'START_SUGGEST', topic });

    try {
      const result = await suggestCouncil(
        { port: serverPort, topic, agent_count: agentCount, model },
      );
      dispatch({
        type: 'SUGGEST_COMPLETE',
        agents: result.agents,
        rounds: result.rounds,
        synthesisGuidance: result.synthesis_guidance,
      });
      return result.agents;
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to suggest council';
      dispatch({ type: 'SUGGEST_ERROR', error: message });
      appLogger.error('hook', 'Council suggest failed', { error: message });
      return null;
    }
  }, [serverPort, model, dispatch]);

  const refine = useCallback(async (instruction: string): Promise<CouncilAgent[] | null> => {
    // Build the previous suggestion from the current session state
    const previousSuggestion = {
      agents: session.suggestedAgents,
      rounds: session.suggestedRounds,
      synthesis_guidance: session.suggestedSynthesisGuidance,
    };

    dispatch({ type: 'START_SUGGEST', topic: session.topic });

    try {
      const result = await suggestCouncil({
        port: serverPort,
        topic: session.topic,
        model,
        previous_suggestion: previousSuggestion,
        refinement: instruction,
      });
      dispatch({
        type: 'SUGGEST_COMPLETE',
        agents: result.agents,
        rounds: result.rounds,
        synthesisGuidance: result.synthesis_guidance,
      });
      return result.agents;
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to refine council';
      dispatch({ type: 'SUGGEST_ERROR', error: message });
      appLogger.error('hook', 'Council refine failed', { error: message });
      return null;
    }
  }, [serverPort, model, session.topic, session.suggestedAgents, session.suggestedRounds, session.suggestedSynthesisGuidance, dispatch]);

  const run = useCallback(async (config: CouncilConfig) => {
    cancel();

    const controller = new AbortController();
    abortRef.current = controller;

    dispatch({ type: 'START_DELIBERATION', topic: config.topic, totalRounds: config.rounds });

    // Build a lookup so we can enrich AGENT_TURN_COMPLETE with metadata
    const agentMap = new Map(config.agents.map((a) => [a.id, a]));

    try {
      await runCouncil(
        { port: serverPort, council: config, model },
        (raw) => {
          const event = raw as unknown as CouncilEvent;
          const action = eventToAction(event);
          if (!action) return;

          // Enrich agent_turn_complete with name/color/contentiousness from config
          if (action.type === 'AGENT_TURN_COMPLETE') {
            const agent = agentMap.get(action.contribution.agentId);
            if (agent) {
              action.contribution.agentName = agent.name;
              action.contribution.color = agent.color;
              action.contribution.contentiousness = agent.contentiousness;
            }
          }

          dispatch(action);
        },
        controller.signal,
      );
    } catch (err) {
      if (controller.signal.aborted) return; // User-initiated cancel
      const message = err instanceof Error ? err.message : 'Council deliberation failed';
      dispatch({ type: 'COUNCIL_ERROR', error: message });
      appLogger.error('hook', 'Council run failed', { error: message });
    } finally {
      if (abortRef.current === controller) {
        abortRef.current = null;
      }
    }
  }, [serverPort, model, cancel, dispatch]);

  const isStreaming = session.phase === 'deliberating' || session.phase === 'synthesizing';

  return { session, suggest, refine, run, cancel, reset, isStreaming };
}
