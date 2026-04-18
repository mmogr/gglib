/**
 * Council session context.
 *
 * Holds the current `CouncilSession` state and exposes dispatch actions
 * to the component tree. The heavy lifting (SSE streaming, API calls)
 * belongs in `useCouncil` — this context is a thin state container.
 *
 * @module contexts/CouncilContext
 */

import { createContext, useContext, useReducer, type Dispatch, type ReactNode } from 'react';
import {
  createEmptySession,
  type CouncilSession,
  type CouncilAgent,
  type AgentContribution,
  type AgentToolCall,
} from '../types/council';

// ─── Actions ────────────────────────────────────────────────────────────────

export type CouncilAction =
  | { type: 'START_SUGGEST'; topic: string }
  | { type: 'START_REFINE' }
  | { type: 'SUGGEST_COMPLETE'; agents: CouncilAgent[]; rounds: number; synthesisGuidance?: string }
  | { type: 'SUGGEST_ERROR'; error: string }
  | { type: 'START_DELIBERATION'; topic: string; totalRounds: number }
  | { type: 'AGENT_TURN_START'; agentId: string; agentName: string; color: string; round: number; contentiousness: number }
  | { type: 'AGENT_TEXT_DELTA'; agentId: string; delta: string }
  | { type: 'AGENT_REASONING_DELTA'; agentId: string; delta: string }
  | { type: 'AGENT_TOOL_CALL_START'; toolCall: AgentToolCall }
  | { type: 'AGENT_TOOL_CALL_COMPLETE'; agentId: string; toolName: string; result: { content: string; isError: boolean }; displayName: string; durationDisplay: string }
  | { type: 'AGENT_TURN_COMPLETE'; contribution: AgentContribution }
  | { type: 'ROUND_SEPARATOR'; round: number }
  | { type: 'JUDGE_START'; round: number }
  | { type: 'JUDGE_TEXT_DELTA'; delta: string }
  | { type: 'JUDGE_SUMMARY'; round: number; summary: string; consensusReached: boolean }
  | { type: 'SYNTHESIS_START' }
  | { type: 'SYNTHESIS_TEXT_DELTA'; delta: string }
  | { type: 'SYNTHESIS_COMPLETE'; content: string }
  | { type: 'COUNCIL_ERROR'; error: string }
  | { type: 'COUNCIL_COMPLETE' }
  | { type: 'UPDATE_AGENT'; agentId: string; changes: Partial<CouncilAgent> }
  | { type: 'ADD_AGENT'; agent: CouncilAgent }
  | { type: 'REMOVE_AGENT'; agentId: string }
  | { type: 'RESET' };

// ─── Reducer ────────────────────────────────────────────────────────────────

export function councilReducer(state: CouncilSession, action: CouncilAction): CouncilSession {
  switch (action.type) {
    case 'START_SUGGEST':
      return { ...createEmptySession(), phase: 'suggesting', topic: action.topic };

    case 'START_REFINE':
      return { ...state, phase: 'suggesting', error: null };

    case 'SUGGEST_COMPLETE':
      return {
        ...state,
        phase: 'setup',
        suggestedAgents: action.agents,
        suggestedRounds: action.rounds,
        suggestedSynthesisGuidance: action.synthesisGuidance,
      };

    case 'SUGGEST_ERROR':
      return { ...state, phase: 'error', error: action.error };

    case 'START_DELIBERATION':
      return {
        ...state,
        phase: 'deliberating',
        topic: action.topic,
        totalRounds: action.totalRounds,
        currentRound: 0,
        contributions: [],
        synthesisText: '',
        error: null,
      };

    case 'AGENT_TURN_START':
      return {
        ...state,
        activeAgentId: action.agentId,
        activeAgentName: action.agentName,
        activeAgentColor: action.color,
        activeAgentContentiousness: action.contentiousness,
        activeAgentText: '',
        activeAgentReasoning: '',
        activeToolCalls: [],
        currentRound: action.round,
      };

    case 'AGENT_TEXT_DELTA':
      if (state.activeAgentId !== action.agentId) return state;
      return { ...state, activeAgentText: state.activeAgentText + action.delta };

    case 'AGENT_REASONING_DELTA':
      if (state.activeAgentId !== action.agentId) return state;
      return { ...state, activeAgentReasoning: state.activeAgentReasoning + action.delta };

    case 'AGENT_TOOL_CALL_START':
      return { ...state, activeToolCalls: [...state.activeToolCalls, action.toolCall] };

    case 'AGENT_TOOL_CALL_COMPLETE':
      return {
        ...state,
        activeToolCalls: state.activeToolCalls.map((tc) =>
          tc.toolName === action.toolName && tc.agentId === action.agentId
            ? { ...tc, result: action.result, displayName: action.displayName, durationDisplay: action.durationDisplay }
            : tc,
        ),
      };

    case 'AGENT_TURN_COMPLETE':
      return {
        ...state,
        activeAgentId: null,
        activeAgentName: '',
        activeAgentColor: '',
        activeAgentContentiousness: 0,
        activeAgentText: '',
        activeAgentReasoning: '',
        activeToolCalls: [],
        contributions: [...state.contributions, action.contribution],
      };

    case 'ROUND_SEPARATOR':
      return { ...state, currentRound: action.round };

    case 'JUDGE_START':
      return { ...state, phase: 'judging', judgeText: '' };

    case 'JUDGE_TEXT_DELTA':
      return { ...state, judgeText: state.judgeText + action.delta };

    case 'JUDGE_SUMMARY':
      return {
        ...state,
        phase: 'deliberating',
        judgeSummary: action.summary,
        judgeConsensusReached: action.consensusReached,
      };

    case 'SYNTHESIS_START':
      return { ...state, phase: 'synthesizing', synthesisText: '' };

    case 'SYNTHESIS_TEXT_DELTA':
      return { ...state, synthesisText: state.synthesisText + action.delta };

    case 'SYNTHESIS_COMPLETE':
      return { ...state, synthesisText: action.content };

    case 'COUNCIL_ERROR':
      return { ...state, phase: 'error', error: action.error };

    case 'COUNCIL_COMPLETE':
      return { ...state, phase: 'complete' };

    case 'UPDATE_AGENT':
      return {
        ...state,
        suggestedAgents: state.suggestedAgents.map((a) =>
          a.id === action.agentId ? { ...a, ...action.changes } : a,
        ),
      };

    case 'ADD_AGENT':
      return { ...state, suggestedAgents: [...state.suggestedAgents, action.agent] };

    case 'REMOVE_AGENT':
      return {
        ...state,
        suggestedAgents: state.suggestedAgents.filter((a) => a.id !== action.agentId),
      };

    case 'RESET':
      return createEmptySession();

    default:
      return state;
  }
}

// ─── Context ────────────────────────────────────────────────────────────────

interface CouncilContextValue {
  session: CouncilSession;
  dispatch: Dispatch<CouncilAction>;
}

const CouncilContext = createContext<CouncilContextValue | null>(null);

export function CouncilProvider({ children }: { children: ReactNode }) {
  const [session, dispatch] = useReducer(councilReducer, undefined, createEmptySession);

  return (
    <CouncilContext.Provider value={{ session, dispatch }}>
      {children}
    </CouncilContext.Provider>
  );
}

/**
 * Access council session state and dispatch.
 *
 * Must be used within a `<CouncilProvider>`.
 */
export function useCouncilContext(): CouncilContextValue {
  const ctx = useContext(CouncilContext);
  if (!ctx) throw new Error('useCouncilContext must be used within <CouncilProvider>');
  return ctx;
}
