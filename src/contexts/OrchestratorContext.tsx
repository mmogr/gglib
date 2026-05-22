/**
 * Orchestrator session context.
 *
 * Holds the current `OrchestratorSession` state and exposes dispatch actions
 * to the component tree. The heavy lifting (SSE streaming, API calls)
 * belongs in `useOrchestrator` — this context is a thin state container.
 *
 * @module contexts/OrchestratorContext
 */

import { createContext, useContext, useReducer, type Dispatch, type ReactNode } from 'react';
import type {
  TaskGraph,
  OrchestratorRun,
  ApprovalKind,
} from '../types/orchestrator';

// ─── Session state ────────────────────────────────────────────────────────────

export type OrchestratorPhase =
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

export interface NodeState {
  phase: NodePhase;
  goal: string;
  text: string;
  toolLog: NodeToolEntry[];
  outputPreview?: string;
  error?: string;
}

export interface PendingApproval {
  approvalId: string;
  kind: ApprovalKind;
  submitting: boolean;
}

export interface OrchestratorSession {
  phase: OrchestratorPhase;
  graph: TaskGraph | null;
  nodeStates: Record<string, NodeState>;
  synthesisText: string;
  finalAnswer: string | null;
  pendingApproval: PendingApproval | null;
  error: string | null;
  /** Runs loaded from GET /api/orchestrator/runs */
  runs: OrchestratorRun[];
  runsLoading: boolean;
}

function createEmptySession(): OrchestratorSession {
  return {
    phase: 'idle',
    graph: null,
    nodeStates: {},
    synthesisText: '',
    finalAnswer: null,
    pendingApproval: null,
    error: null,
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
  | { type: 'RUNS_LOADED'; runs: OrchestratorRun[] }
  | { type: 'RUNS_ERROR' };

// ─── Reducer ──────────────────────────────────────────────────────────────────

export function orchestratorReducer(
  state: OrchestratorSession,
  action: OrchestratorAction,
): OrchestratorSession {
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

    default:
      return state;
  }
}

// ─── Context ──────────────────────────────────────────────────────────────────

interface OrchestratorContextValue {
  session: OrchestratorSession;
  dispatch: Dispatch<OrchestratorAction>;
}

const OrchestratorContext = createContext<OrchestratorContextValue | null>(null);

export function OrchestratorProvider({ children }: { children: ReactNode }) {
  const [session, dispatch] = useReducer(orchestratorReducer, undefined, createEmptySession);

  return (
    <OrchestratorContext.Provider value={{ session, dispatch }}>
      {children}
    </OrchestratorContext.Provider>
  );
}

/**
 * Access orchestrator session state and dispatch.
 *
 * Must be used within an `<OrchestratorProvider>`.
 */
export function useOrchestratorContext(): OrchestratorContextValue {
  const ctx = useContext(OrchestratorContext);
  if (!ctx) throw new Error('useOrchestratorContext must be used within <OrchestratorProvider>');
  return ctx;
}
