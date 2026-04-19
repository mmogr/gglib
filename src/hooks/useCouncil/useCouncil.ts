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
  /** Update a single agent's properties. */
  updateAgent: (agentId: string, changes: Partial<CouncilAgent>) => void;
  /** Remove an agent by id. */
  removeAgent: (agentId: string) => void;
  /** Add a blank agent scaffold. */
  addAgent: () => void;
  /** Ask the LLM to fill/update a single agent's details by name. */
  fillAgent: (agentId: string) => Promise<void>;
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
    case 'judge_start':
      return { type: 'JUDGE_START', round: event.round };
    case 'judge_text_delta':
      return { type: 'JUDGE_TEXT_DELTA', delta: event.delta };
    case 'judge_summary':
      return {
        type: 'JUDGE_SUMMARY',
        round: event.round,
        summary: event.summary,
        consensusReached: event.consensus_reached,
      };
    case 'round_compacted':
      return { type: 'ROUND_COMPACTED', round: event.round, summary: event.summary };
    case 'stance_map':
      return { type: 'STANCE_MAP', stances: event.stances };
    case 'synthesis_start':
      return { type: 'SYNTHESIS_START' };
    case 'synthesis_progress':
      return { type: 'SYNTHESIS_PROGRESS', processed: event.processed, total: event.total, cached: event.cached, timeMs: event.time_ms };
    case 'agent_progress':
      return { type: 'AGENT_PROGRESS', agentId: event.agent_id, processed: event.processed, total: event.total, cached: event.cached, timeMs: event.time_ms };
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

    dispatch({ type: 'START_REFINE' });

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

  const updateAgent = useCallback((agentId: string, changes: Partial<CouncilAgent>) => {
    dispatch({ type: 'UPDATE_AGENT', agentId, changes });
  }, [dispatch]);

  const removeAgent = useCallback((agentId: string) => {
    dispatch({ type: 'REMOVE_AGENT', agentId });
  }, [dispatch]);

  const addAgent = useCallback(() => {
    const id = `new-agent-${Date.now()}`;
    const colors = ['#3b82f6','#ef4444','#10b981','#f59e0b','#8b5cf6','#ec4899','#06b6d4','#f97316'];
    const idx = session.suggestedAgents.length % colors.length;
    dispatch({
      type: 'ADD_AGENT',
      agent: {
        id,
        name: 'New Agent',
        color: colors[idx],
        persona: 'Define this agent\'s worldview and expertise.',
        perspective: 'Describe their unique angle.',
        contentiousness: 0.5,
      },
    });
  }, [dispatch, session.suggestedAgents.length]);

  const fillAgent = useCallback(async (agentId: string): Promise<void> => {
    const target = session.suggestedAgents.find((a) => a.id === agentId);
    if (!target) return;
    // Send only agent names as context — avoids echoing full personas back.
    const roster = session.suggestedAgents.map((a) => a.name).join(', ');
    let result;
    try {
      result = await suggestCouncil({
        port: serverPort,
        topic: session.topic,
        model,
        agent_count: 1,
        refinement: `The council already has these agents: [${roster}]. `
          + `Generate details for the agent named '${target.name}' to complement them. `
          + `Return a JSON with ONLY this one agent in the "agents" array — do NOT `
          + `regenerate the other agents. Include id, name, persona (2-3 sentences), `
          + `perspective (1 sentence), contentiousness (0.0-1.0), rounds, and synthesis_guidance.`,
      });
    } catch (err) {
      appLogger.error('hook', 'Council fill failed', { error: err instanceof Error ? err.message : String(err) });
      return;
    }
    const filled = result.agents.find((a) => a.name === target.name) ?? result.agents[0];
    if (!filled) return;
    dispatch({
      type: 'UPDATE_AGENT',
      agentId,
      changes: { persona: filled.persona, perspective: filled.perspective, contentiousness: filled.contentiousness },
    });
  }, [serverPort, model, session.topic, session.suggestedAgents, dispatch]);

  return { session, suggest, refine, run, cancel, reset, updateAgent, removeAgent, addAgent, fillAgent, isStreaming };
}
