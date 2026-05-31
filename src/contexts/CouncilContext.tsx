/**
 * Orchestrator session context.
 *
 * Holds the current `CouncilSession` state and exposes dispatch actions
 * to the component tree. The heavy lifting (SSE streaming, API calls)
 * belongs in `useCouncil` — this context is a thin state container.
 *
 * @module contexts/CouncilContext
 */

import { createContext, useContext, useReducer, type Dispatch, type ReactNode } from 'react';
import type {
  TaskGraph,
  CouncilRun,
  ApprovalKind,
  GraphDiff,
  AgentStance,
} from '../types/council';

// ─── Cost estimate ───────────────────────────────────────────────────────────────────

export interface RunCostEstimate {
  nodeCount: number;
  estTokens: number;
  estWallSeconds: number;
}

// ─── Session state ────────────────────────────────────────────────────────────

export type CouncilPhase =
  | 'idle'
  | 'planning'
  | 'running'
  | 'awaiting_approval'
  | 'synthesizing'
  | 'complete'
  | 'error';

export type NodePhase = 'pending' | 'running' | 'compacting' | 'done' | 'failed';

export interface NodeToolEntry {
  displayName: string;
  argsSummary: string;
  durationDisplay?: string;
  done: boolean;
}

// ─── Debate sub-state ─────────────────────────────────────────────────────────

export interface DebateAgentTurn {
  agentId: string;
  agentName: string;
  color: string;
  /** Accumulating text as delta events arrive. */
  text: string;
  done: boolean;
  finalText?: string;
  toolLog: NodeToolEntry[];
}

export interface DebateJudgeSummaryState {
  round: number;
  assessmentText: string;
  consensusReached: boolean;
  earlyStopRecommended: boolean;
}

export interface DebateRoundState {
  round: number;
  /** Keyed by agentId. */
  turns: Record<string, DebateAgentTurn>;
  /** Accumulating judge text for this round (before summary arrives). */
  judgeText: string;
  judgeSummary?: DebateJudgeSummaryState;
  compacted?: boolean;
  compactedSummary?: string;
}

export interface DebateSynthesisState {
  started: boolean;
  text: string;
  done: boolean;
  finalText?: string;
}

export interface DebateNodeState {
  /** Ordered list of rounds; index 0 = round 1. */
  rounds: DebateRoundState[];
  stances: AgentStance[];
  synthesis: DebateSynthesisState;
}

export interface NodeState {
  phase: NodePhase;
  goal: string;
  text: string;
  toolLog: NodeToolEntry[];
  outputPreview?: string;
  error?: string;
  /** Populated only for `debate`-kind nodes. */
  debateState?: DebateNodeState | null;
}

export interface PendingApproval {
  approvalId: string;
  kind: ApprovalKind;
  submitting: boolean;
}

export interface CouncilSession {
  phase: CouncilPhase;
  graph: TaskGraph | null;
  nodeStates: Record<string, NodeState>;
  synthesisText: string;
  finalAnswer: string | null;
  pendingApproval: PendingApproval | null;
  costEstimate: RunCostEstimate | null;
  error: string | null;
  /** Most recent steering diff received from a steering_applied event. */
  pendingDiff: GraphDiff | null;
  /** Runs loaded from GET /api/council/runs */
  runs: CouncilRun[];
  runsLoading: boolean;
}

function createEmptySession(): CouncilSession {
  return {
    phase: 'idle',
    graph: null,
    nodeStates: {},
    synthesisText: '',
    finalAnswer: null,
    pendingApproval: null,
    costEstimate: null,
    error: null,
    pendingDiff: null,
    runs: [],
    runsLoading: false,
  };
}

// ─── Actions ──────────────────────────────────────────────────────────────────

export type OrchestratorAction =
  // Lifecycle
  | { type: 'START_RUN'; goal: string }
  | { type: 'RESET' }
  // Planning
  | { type: 'PLAN_PROPOSED'; graph: TaskGraph }
  | { type: 'PLAN_APPROVED' }
  | { type: 'PLAN_REJECTED'; reason?: string | null }
  | { type: 'REPLAN_ATTEMPT'; attempt: number; reason: string }
  | { type: 'SET_COST_ESTIMATE'; nodeCount: number; estTokens: number; estWallSeconds: number }
  // Node lifecycle
  | { type: 'NODE_STARTED'; nodeId: string; goal: string }
  | { type: 'NODE_TEXT_DELTA'; nodeId: string; delta: string }
  | { type: 'NODE_TOOL_CALL_START'; nodeId: string; displayName: string; argsSummary: string }
  | { type: 'NODE_TOOL_CALL_COMPLETE'; nodeId: string; toolName: string; displayName: string; durationDisplay: string }
  | { type: 'NODE_COMPACTING'; nodeId: string }
  | { type: 'NODE_COMPLETE'; nodeId: string; outputPreview: string }
  | { type: 'NODE_FAILED'; nodeId: string; error: string }
  // Synthesis
  | { type: 'SYNTHESIS_START' }
  | { type: 'SYNTHESIS_TEXT_DELTA'; delta: string }
  | { type: 'SYNTHESIS_COMPLETE'; content: string }
  // HITL approval
  | { type: 'AWAITING_APPROVAL'; approvalId: string; kind: ApprovalKind }
  | { type: 'APPROVAL_SUBMITTING' }
  | { type: 'APPROVAL_DONE' }
  // Terminal
  | { type: 'ORCHESTRATOR_COMPLETE'; answer: string }
  | { type: 'ORCHESTRATOR_ERROR'; message: string }
  // Runs list
  | { type: 'RUNS_LOADING' }
  | { type: 'RUNS_LOADED'; runs: CouncilRun[] }
  | { type: 'RUNS_ERROR' }
  // Steering (Phase K)
  | { type: 'SET_PENDING_DIFF'; diff: GraphDiff | null }
  // Debate events (Phase N)
  | { type: 'DEBATE_ROUND_STARTED'; nodeId: string; round: number }
  | { type: 'DEBATE_AGENT_TURN_STARTED'; nodeId: string; agentId: string; agentName: string; color: string; round: number; contentiousness: number }
  | { type: 'DEBATE_AGENT_TEXT_DELTA'; nodeId: string; agentId: string; delta: string }
  | { type: 'DEBATE_AGENT_TOOL_CALL_START'; nodeId: string; agentId: string; displayName: string; argsSummary: string | null | undefined }
  | { type: 'DEBATE_AGENT_TOOL_CALL_COMPLETE'; nodeId: string; agentId: string; displayName: string; durationDisplay: string }
  | { type: 'DEBATE_AGENT_TURN_COMPLETE'; nodeId: string; agentId: string; round: number; finalText: string }
  | { type: 'DEBATE_JUDGE_STARTED'; nodeId: string; round: number }
  | { type: 'DEBATE_JUDGE_TEXT_DELTA'; nodeId: string; delta: string }
  | { type: 'DEBATE_JUDGE_SUMMARY'; nodeId: string; round: number; consensusReached: boolean; earlyStopRecommended: boolean; assessmentText: string }
  | { type: 'DEBATE_ROUND_COMPACTED'; nodeId: string; round: number; summary: string }
  | { type: 'DEBATE_STANCE_MAP'; nodeId: string; stances: AgentStance[] }
  | { type: 'DEBATE_SYNTHESIS_STARTED'; nodeId: string }
  | { type: 'DEBATE_SYNTHESIS_TEXT_DELTA'; nodeId: string; delta: string }
  | { type: 'DEBATE_SYNTHESIS_COMPLETE'; nodeId: string; finalText: string };

// ─── Reducer ──────────────────────────────────────────────────────────────────

export function orchestratorReducer(
  state: CouncilSession,
  action: OrchestratorAction,
): CouncilSession {
  switch (action.type) {
    case 'START_RUN':
      return {
        ...createEmptySession(),
        phase: 'running',
        runs: state.runs,
        runsLoading: state.runsLoading,
      };

    case 'RESET':
      return {
        ...createEmptySession(),
        runs: state.runs,
        runsLoading: state.runsLoading,
      };

    case 'PLAN_PROPOSED':
      return { ...state, graph: action.graph };

    case 'SET_COST_ESTIMATE':
      return {
        ...state,
        costEstimate: {
          nodeCount: action.nodeCount,
          estTokens: action.estTokens,
          estWallSeconds: action.estWallSeconds,
        },
      };

    case 'PLAN_APPROVED':
      return { ...state, pendingApproval: null };

    case 'PLAN_REJECTED':
      return { ...state, pendingApproval: null };

    case 'REPLAN_ATTEMPT':
      return state;

    case 'NODE_STARTED':
      return {
        ...state,
        nodeStates: {
          ...state.nodeStates,
          [action.nodeId]: {
            phase: 'running',
            goal: action.goal,
            text: '',
            toolLog: [],
          },
        },
      };

    case 'NODE_TEXT_DELTA': {
      const ns = state.nodeStates[action.nodeId];
      if (!ns) return state;
      return {
        ...state,
        nodeStates: {
          ...state.nodeStates,
          [action.nodeId]: { ...ns, text: ns.text + action.delta },
        },
      };
    }

    case 'NODE_TOOL_CALL_START': {
      const ns = state.nodeStates[action.nodeId];
      if (!ns) return state;
      return {
        ...state,
        nodeStates: {
          ...state.nodeStates,
          [action.nodeId]: {
            ...ns,
            toolLog: [
              ...ns.toolLog,
              { displayName: action.displayName, argsSummary: action.argsSummary, done: false },
            ],
          },
        },
      };
    }

    case 'NODE_TOOL_CALL_COMPLETE': {
      const ns = state.nodeStates[action.nodeId];
      if (!ns) return state;
      // Mark the last entry with this displayName as done
      const toolLog = [...ns.toolLog];
      const idx = toolLog.map((t) => t.displayName).lastIndexOf(action.displayName);
      if (idx !== -1) {
        toolLog[idx] = { ...toolLog[idx], done: true, durationDisplay: action.durationDisplay };
      }
      return {
        ...state,
        nodeStates: { ...state.nodeStates, [action.nodeId]: { ...ns, toolLog } },
      };
    }

    case 'NODE_COMPACTING': {
      const ns = state.nodeStates[action.nodeId];
      if (!ns) return state;
      return {
        ...state,
        nodeStates: { ...state.nodeStates, [action.nodeId]: { ...ns, phase: 'compacting' } },
      };
    }

    case 'NODE_COMPLETE': {
      const ns = state.nodeStates[action.nodeId];
      if (!ns) return state;
      return {
        ...state,
        nodeStates: {
          ...state.nodeStates,
          [action.nodeId]: { ...ns, phase: 'done', outputPreview: action.outputPreview },
        },
      };
    }

    case 'NODE_FAILED': {
      const ns = state.nodeStates[action.nodeId];
      if (!ns) return state;
      return {
        ...state,
        nodeStates: {
          ...state.nodeStates,
          [action.nodeId]: { ...ns, phase: 'failed', error: action.error },
        },
      };
    }

    case 'SYNTHESIS_START':
      return { ...state, phase: 'synthesizing', synthesisText: '' };

    case 'SYNTHESIS_TEXT_DELTA':
      return { ...state, synthesisText: state.synthesisText + action.delta };

    case 'SYNTHESIS_COMPLETE':
      return { ...state, synthesisText: action.content };

    case 'AWAITING_APPROVAL':
      return {
        ...state,
        phase: 'awaiting_approval',
        pendingApproval: { approvalId: action.approvalId, kind: action.kind, submitting: false },
      };

    case 'APPROVAL_SUBMITTING':
      if (!state.pendingApproval) return state;
      return {
        ...state,
        pendingApproval: { ...state.pendingApproval, submitting: true },
      };

    case 'APPROVAL_DONE':
      return { ...state, phase: 'running', pendingApproval: null };

    case 'ORCHESTRATOR_COMPLETE':
      return { ...state, phase: 'complete', finalAnswer: action.answer };

    case 'ORCHESTRATOR_ERROR':
      return { ...state, phase: 'error', error: action.message };

    case 'RUNS_LOADING':
      return { ...state, runsLoading: true };

    case 'RUNS_LOADED':
      return { ...state, runs: action.runs, runsLoading: false };

    case 'RUNS_ERROR':
      return { ...state, runsLoading: false };

    case 'SET_PENDING_DIFF':
      return { ...state, pendingDiff: action.diff };

    // ── Debate events (Phase N) ────────────────────────────────────────────

    case 'DEBATE_ROUND_STARTED': {
      const ns = state.nodeStates[action.nodeId];
      if (!ns) return state;
      const prev = ns.debateState ?? { rounds: [], stances: [], synthesis: { started: false, text: '', done: false } };
      const newRound: DebateRoundState = { round: action.round, turns: {}, judgeText: '' };
      return {
        ...state,
        nodeStates: {
          ...state.nodeStates,
          [action.nodeId]: {
            ...ns,
            debateState: { ...prev, rounds: [...prev.rounds, newRound] },
          },
        },
      };
    }

    case 'DEBATE_AGENT_TURN_STARTED': {
      const ns = state.nodeStates[action.nodeId];
      if (!ns || !ns.debateState) return state;
      const ds = ns.debateState;
      const roundIdx = ds.rounds.findIndex(r => r.round === action.round);
      if (roundIdx === -1) return state;
      const round = ds.rounds[roundIdx];
      const newTurn: DebateAgentTurn = {
        agentId: action.agentId, agentName: action.agentName,
        color: action.color, text: '', done: false, toolLog: [],
      };
      const updatedRound: DebateRoundState = {
        ...round,
        turns: { ...round.turns, [action.agentId]: newTurn },
      };
      const rounds = [...ds.rounds];
      rounds[roundIdx] = updatedRound;
      return {
        ...state,
        nodeStates: {
          ...state.nodeStates,
          [action.nodeId]: { ...ns, debateState: { ...ds, rounds } },
        },
      };
    }

    case 'DEBATE_AGENT_TEXT_DELTA': {
      const ns = state.nodeStates[action.nodeId];
      if (!ns || !ns.debateState) return state;
      const ds = ns.debateState;
      // Find the latest non-done round containing this agent's turn
      const roundIdx = [...ds.rounds].reverse().findIndex(r => r.turns[action.agentId] && !r.turns[action.agentId].done);
      if (roundIdx === -1) return state;
      const realIdx = ds.rounds.length - 1 - roundIdx;
      const round = ds.rounds[realIdx];
      const turn = round.turns[action.agentId];
      const updatedTurn: DebateAgentTurn = { ...turn, text: turn.text + action.delta };
      const rounds = [...ds.rounds];
      rounds[realIdx] = { ...round, turns: { ...round.turns, [action.agentId]: updatedTurn } };
      return {
        ...state,
        nodeStates: {
          ...state.nodeStates,
          [action.nodeId]: { ...ns, debateState: { ...ds, rounds } },
        },
      };
    }

    case 'DEBATE_AGENT_TOOL_CALL_START': {
      const ns = state.nodeStates[action.nodeId];
      if (!ns || !ns.debateState) return state;
      const ds = ns.debateState;
      const roundIdx = [...ds.rounds].map((r, i) => [r, i] as const).reverse().find(([r]) => r.turns[action.agentId] && !r.turns[action.agentId].done)?.[1];
      if (roundIdx === undefined) return state;
      const round = ds.rounds[roundIdx];
      const turn = round.turns[action.agentId];
      const newEntry: NodeToolEntry = {
        displayName: action.displayName,
        argsSummary: action.argsSummary ?? '',
        done: false,
      };
      const updatedTurn: DebateAgentTurn = { ...turn, toolLog: [...turn.toolLog, newEntry] };
      const rounds = [...ds.rounds];
      rounds[roundIdx] = { ...round, turns: { ...round.turns, [action.agentId]: updatedTurn } };
      return {
        ...state,
        nodeStates: {
          ...state.nodeStates,
          [action.nodeId]: { ...ns, debateState: { ...ds, rounds } },
        },
      };
    }

    case 'DEBATE_AGENT_TOOL_CALL_COMPLETE': {
      const ns = state.nodeStates[action.nodeId];
      if (!ns || !ns.debateState) return state;
      const ds = ns.debateState;
      const roundIdx = [...ds.rounds].map((r, i) => [r, i] as const).reverse().find(([r]) => r.turns[action.agentId] && !r.turns[action.agentId].done)?.[1];
      if (roundIdx === undefined) return state;
      const round = ds.rounds[roundIdx];
      const turn = round.turns[action.agentId];
      const toolLog = [...turn.toolLog];
      const idx = toolLog.map(t => t.displayName).lastIndexOf(action.displayName);
      if (idx !== -1) {
        toolLog[idx] = { ...toolLog[idx], done: true, durationDisplay: action.durationDisplay };
      }
      const rounds = [...ds.rounds];
      rounds[roundIdx] = { ...round, turns: { ...round.turns, [action.agentId]: { ...turn, toolLog } } };
      return {
        ...state,
        nodeStates: {
          ...state.nodeStates,
          [action.nodeId]: { ...ns, debateState: { ...ds, rounds } },
        },
      };
    }

    case 'DEBATE_AGENT_TURN_COMPLETE': {
      const ns = state.nodeStates[action.nodeId];
      if (!ns || !ns.debateState) return state;
      const ds = ns.debateState;
      const roundIdx = ds.rounds.findIndex(r => r.round === action.round);
      if (roundIdx === -1) return state;
      const round = ds.rounds[roundIdx];
      const turn = round.turns[action.agentId];
      if (!turn) return state;
      const updatedTurn: DebateAgentTurn = { ...turn, done: true, finalText: action.finalText };
      const rounds = [...ds.rounds];
      rounds[roundIdx] = { ...round, turns: { ...round.turns, [action.agentId]: updatedTurn } };
      return {
        ...state,
        nodeStates: {
          ...state.nodeStates,
          [action.nodeId]: { ...ns, debateState: { ...ds, rounds } },
        },
      };
    }

    case 'DEBATE_JUDGE_STARTED': {
      const ns = state.nodeStates[action.nodeId];
      if (!ns || !ns.debateState) return state;
      const ds = ns.debateState;
      const roundIdx = ds.rounds.findIndex(r => r.round === action.round);
      if (roundIdx === -1) return state;
      const rounds = [...ds.rounds];
      rounds[roundIdx] = { ...rounds[roundIdx], judgeText: '' };
      return {
        ...state,
        nodeStates: {
          ...state.nodeStates,
          [action.nodeId]: { ...ns, debateState: { ...ds, rounds } },
        },
      };
    }

    case 'DEBATE_JUDGE_TEXT_DELTA': {
      const ns = state.nodeStates[action.nodeId];
      if (!ns || !ns.debateState) return state;
      const ds = ns.debateState;
      if (ds.rounds.length === 0) return state;
      const lastIdx = ds.rounds.length - 1;
      const rounds = [...ds.rounds];
      rounds[lastIdx] = { ...rounds[lastIdx], judgeText: rounds[lastIdx].judgeText + action.delta };
      return {
        ...state,
        nodeStates: {
          ...state.nodeStates,
          [action.nodeId]: { ...ns, debateState: { ...ds, rounds } },
        },
      };
    }

    case 'DEBATE_JUDGE_SUMMARY': {
      const ns = state.nodeStates[action.nodeId];
      if (!ns || !ns.debateState) return state;
      const ds = ns.debateState;
      const roundIdx = ds.rounds.findIndex(r => r.round === action.round);
      if (roundIdx === -1) return state;
      const rounds = [...ds.rounds];
      rounds[roundIdx] = {
        ...rounds[roundIdx],
        judgeSummary: {
          round: action.round,
          assessmentText: action.assessmentText,
          consensusReached: action.consensusReached,
          earlyStopRecommended: action.earlyStopRecommended,
        },
      };
      return {
        ...state,
        nodeStates: {
          ...state.nodeStates,
          [action.nodeId]: { ...ns, debateState: { ...ds, rounds } },
        },
      };
    }

    case 'DEBATE_ROUND_COMPACTED': {
      const ns = state.nodeStates[action.nodeId];
      if (!ns || !ns.debateState) return state;
      const ds = ns.debateState;
      const roundIdx = ds.rounds.findIndex(r => r.round === action.round);
      if (roundIdx === -1) return state;
      const rounds = [...ds.rounds];
      rounds[roundIdx] = { ...rounds[roundIdx], compacted: true, compactedSummary: action.summary };
      return {
        ...state,
        nodeStates: {
          ...state.nodeStates,
          [action.nodeId]: { ...ns, debateState: { ...ds, rounds } },
        },
      };
    }

    case 'DEBATE_STANCE_MAP': {
      const ns = state.nodeStates[action.nodeId];
      if (!ns || !ns.debateState) return state;
      return {
        ...state,
        nodeStates: {
          ...state.nodeStates,
          [action.nodeId]: {
            ...ns,
            debateState: { ...ns.debateState, stances: action.stances },
          },
        },
      };
    }

    case 'DEBATE_SYNTHESIS_STARTED': {
      const ns = state.nodeStates[action.nodeId];
      if (!ns || !ns.debateState) return state;
      return {
        ...state,
        nodeStates: {
          ...state.nodeStates,
          [action.nodeId]: {
            ...ns,
            debateState: { ...ns.debateState, synthesis: { started: true, text: '', done: false } },
          },
        },
      };
    }

    case 'DEBATE_SYNTHESIS_TEXT_DELTA': {
      const ns = state.nodeStates[action.nodeId];
      if (!ns || !ns.debateState) return state;
      const prev = ns.debateState.synthesis;
      return {
        ...state,
        nodeStates: {
          ...state.nodeStates,
          [action.nodeId]: {
            ...ns,
            debateState: { ...ns.debateState, synthesis: { ...prev, text: prev.text + action.delta } },
          },
        },
      };
    }

    case 'DEBATE_SYNTHESIS_COMPLETE': {
      const ns = state.nodeStates[action.nodeId];
      if (!ns || !ns.debateState) return state;
      const prev = ns.debateState.synthesis;
      return {
        ...state,
        nodeStates: {
          ...state.nodeStates,
          [action.nodeId]: {
            ...ns,
            debateState: {
              ...ns.debateState,
              synthesis: { ...prev, done: true, finalText: action.finalText },
            },
          },
        },
      };
    }

    default:
      return state;
  }
}

// ─── Context ──────────────────────────────────────────────────────────────────

interface CouncilContextValue {
  session: CouncilSession;
  dispatch: Dispatch<OrchestratorAction>;
}

const CouncilContext = createContext<CouncilContextValue | null>(null);

export function CouncilProvider({ children }: { children: ReactNode }) {
  const [session, dispatch] = useReducer(orchestratorReducer, undefined, createEmptySession);

  return (
    <CouncilContext.Provider value={{ session, dispatch }}>
      {children}
    </CouncilContext.Provider>
  );
}

/**
 * Access orchestrator session state and dispatch.
 *
 * Must be used within an `<CouncilProvider>`.
 */
export function useCouncilContext(): CouncilContextValue {
  const ctx = useContext(CouncilContext);
  if (!ctx) throw new Error('useCouncilContext must be used within <CouncilProvider>');
  return ctx;
}
