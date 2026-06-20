/**
 * Tests for OrchestratorContext reducer state transitions.
 */

import { describe, it, expect } from 'vitest';
import {
  orchestratorReducer,
  type CouncilSession,
  type OrchestratorAction,
} from '../../../src/contexts/CouncilContext';
import type { TaskGraph, ApprovalKind } from '../../../src/types/council';

const EMPTY_GRAPH: TaskGraph = {
  goal: 'test goal',
  hitl_mode: 'none',
  nodes: {
    t1: { id: 't1', goal: 'do task 1', depends_on: [], tool_allowlist: [], status: 'pending' },
    t2: { id: 't2', goal: 'do task 2', depends_on: ['t1'], tool_allowlist: ['search'], status: 'pending' },
  },
};

function emptySession(): CouncilSession {
  return makeSession();
}

// Build an initial idle session via RESET from a fabricated state
function makeSession(overrides: Partial<CouncilSession> = {}): CouncilSession {
  return {
    phase: 'idle',
    graph: null,
    nodeStates: {},
    synthesisText: '',
    finalAnswer: null,
    pendingApproval: null,
    costEstimate: null,
    pendingDiff: null,
    error: null,
    runs: [],
    runsLoading: false,
    ...overrides,
  };
}

describe('orchestratorReducer', () => {
  it('RESET returns idle session', () => {
    const state = makeSession({ phase: 'complete', finalAnswer: 'answer', graph: EMPTY_GRAPH });
    const next = orchestratorReducer(state, { type: 'RESET' });
    expect(next.phase).toBe('idle');
    expect(next.graph).toBeNull();
    expect(next.finalAnswer).toBeNull();
  });

  it('START_RUN moves phase to running and clears session', () => {
    const state = makeSession({ phase: 'idle', runs: [{ id: 'r1', goal: 'g', graph_json: null, status: 'completed', hitl_mode: 'none', conversation_id: null, created_at: '', updated_at: '' }] });
    const next = orchestratorReducer(state, { type: 'START_RUN', goal: 'new goal' });
    expect(next.phase).toBe('running');
    expect(next.graph).toBeNull();
    expect(next.synthesisText).toBe('');
    // preserves runs
    expect(next.runs).toHaveLength(1);
  });

  it('PLAN_PROPOSED sets graph', () => {
    const state = makeSession({ phase: 'running' });
    const next = orchestratorReducer(state, { type: 'PLAN_PROPOSED', graph: EMPTY_GRAPH });
    expect(next.graph).toEqual(EMPTY_GRAPH);
  });

  it('NODE_STARTED creates node state', () => {
    const state = makeSession({ phase: 'running' });
    const next = orchestratorReducer(state, { type: 'NODE_STARTED', nodeId: 't1', goal: 'do task 1' });
    expect(next.nodeStates['t1']).toEqual({ phase: 'running', goal: 'do task 1', text: '', toolLog: [] });
  });

  it('NODE_TEXT_DELTA accumulates text', () => {
    const state = makeSession({
      nodeStates: { t1: { phase: 'running', goal: 'g', text: 'Hello', toolLog: [] } },
    });
    const next = orchestratorReducer(state, { type: 'NODE_TEXT_DELTA', nodeId: 't1', delta: ' World' });
    expect(next.nodeStates['t1'].text).toBe('Hello World');
  });

  it('NODE_TEXT_DELTA ignores unknown node', () => {
    const state = makeSession({ nodeStates: {} });
    const next = orchestratorReducer(state, { type: 'NODE_TEXT_DELTA', nodeId: 'unknown', delta: 'x' });
    expect(next).toBe(state);
  });

  it('NODE_TOOL_CALL_START appends to toolLog', () => {
    const state = makeSession({
      nodeStates: { t1: { phase: 'running', goal: 'g', text: '', toolLog: [] } },
    });
    const next = orchestratorReducer(state, {
      type: 'NODE_TOOL_CALL_START',
      nodeId: 't1',
      displayName: 'search',
      argsSummary: 'query=foo',
    });
    expect(next.nodeStates['t1'].toolLog).toHaveLength(1);
    expect(next.nodeStates['t1'].toolLog[0].done).toBe(false);
    expect(next.nodeStates['t1'].toolLog[0].displayName).toBe('search');
  });

  it('NODE_TOOL_CALL_COMPLETE marks tool done', () => {
    const state = makeSession({
      nodeStates: {
        t1: {
          phase: 'running', goal: 'g', text: '',
          toolLog: [{ displayName: 'search', argsSummary: 'q', done: false }],
        },
      },
    });
    const next = orchestratorReducer(state, {
      type: 'NODE_TOOL_CALL_COMPLETE',
      nodeId: 't1',
      toolName: 'search',
      displayName: 'search',
      durationDisplay: '1.2s',
    });
    expect(next.nodeStates['t1'].toolLog[0].done).toBe(true);
    expect(next.nodeStates['t1'].toolLog[0].durationDisplay).toBe('1.2s');
  });

  it('NODE_COMPLETE sets phase done and outputPreview', () => {
    const state = makeSession({
      nodeStates: { t1: { phase: 'running', goal: 'g', text: 'some text', toolLog: [] } },
    });
    const next = orchestratorReducer(state, {
      type: 'NODE_COMPLETE',
      nodeId: 't1',
      outputPreview: 'preview here',
    });
    expect(next.nodeStates['t1'].phase).toBe('done');
    expect(next.nodeStates['t1'].outputPreview).toBe('preview here');
  });

  it('NODE_FAILED sets phase failed and error', () => {
    const state = makeSession({
      nodeStates: { t1: { phase: 'running', goal: 'g', text: '', toolLog: [] } },
    });
    const next = orchestratorReducer(state, { type: 'NODE_FAILED', nodeId: 't1', error: 'timeout' });
    expect(next.nodeStates['t1'].phase).toBe('failed');
    expect(next.nodeStates['t1'].error).toBe('timeout');
  });

  it('NODE_COMPACTING sets phase compacting', () => {
    const state = makeSession({
      nodeStates: { t1: { phase: 'running', goal: 'g', text: 'long text', toolLog: [] } },
    });
    const next = orchestratorReducer(state, { type: 'NODE_COMPACTING', nodeId: 't1' });
    expect(next.nodeStates['t1'].phase).toBe('compacting');
  });

  it('SYNTHESIS_START sets phase synthesizing and clears text', () => {
    const state = makeSession({ synthesisText: 'old', phase: 'running' });
    const next = orchestratorReducer(state, { type: 'SYNTHESIS_START' });
    expect(next.phase).toBe('synthesizing');
    expect(next.synthesisText).toBe('');
  });

  it('SYNTHESIS_TEXT_DELTA accumulates synthesis text', () => {
    const state = makeSession({ phase: 'synthesizing', synthesisText: 'Hello' });
    const next = orchestratorReducer(state, { type: 'SYNTHESIS_TEXT_DELTA', delta: ' world' });
    expect(next.synthesisText).toBe('Hello world');
  });

  it('ORCHESTRATOR_COMPLETE sets phase complete and finalAnswer', () => {
    const state = makeSession({ phase: 'synthesizing' });
    const next = orchestratorReducer(state, { type: 'ORCHESTRATOR_COMPLETE', answer: 'the answer' });
    expect(next.phase).toBe('complete');
    expect(next.finalAnswer).toBe('the answer');
  });

  it('ORCHESTRATOR_ERROR sets phase error and error message', () => {
    const state = makeSession({ phase: 'running' });
    const next = orchestratorReducer(state, { type: 'ORCHESTRATOR_ERROR', message: 'something went wrong' });
    expect(next.phase).toBe('error');
    expect(next.error).toBe('something went wrong');
  });

  it('AWAITING_APPROVAL sets phase and pendingApproval', () => {
    const state = makeSession({ phase: 'running' });
    const kind: ApprovalKind = { kind: 'plan' };
    const next = orchestratorReducer(state, { type: 'AWAITING_APPROVAL', approvalId: 'abc', kind });
    expect(next.phase).toBe('awaiting_approval');
    expect(next.pendingApproval).toEqual({ approvalId: 'abc', kind, submitting: false });
  });

  it('APPROVAL_SUBMITTING marks submitting true', () => {
    const state = makeSession({
      pendingApproval: { approvalId: 'abc', kind: { kind: 'plan' }, submitting: false },
    });
    const next = orchestratorReducer(state, { type: 'APPROVAL_SUBMITTING' });
    expect(next.pendingApproval?.submitting).toBe(true);
  });

  it('APPROVAL_DONE clears pendingApproval and resumes running', () => {
    const state = makeSession({
      phase: 'awaiting_approval',
      pendingApproval: { approvalId: 'abc', kind: { kind: 'plan' }, submitting: true },
    });
    const next = orchestratorReducer(state, { type: 'APPROVAL_DONE' });
    expect(next.phase).toBe('running');
    expect(next.pendingApproval).toBeNull();
  });

  it('RUNS_LOADED stores runs and clears loading', () => {
    const state = makeSession({ runsLoading: true, runs: [] });
    const runs = [{ id: 'r1', goal: 'g', graph_json: null, status: 'completed' as const, hitl_mode: 'none' as const, conversation_id: null, created_at: '', updated_at: '' }];
    const next = orchestratorReducer(state, { type: 'RUNS_LOADED', runs });
    expect(next.runs).toHaveLength(1);
    expect(next.runsLoading).toBe(false);
  });

  describe('session preserved across RESET', () => {
    it('RESET preserves runs from previous state', () => {
      const runs = [{ id: 'r1', goal: 'g', graph_json: null, status: 'completed' as const, hitl_mode: 'none' as const, conversation_id: null, created_at: '', updated_at: '' }];
      const state = makeSession({ phase: 'complete', runs });
      const next = orchestratorReducer(state, { type: 'RESET' });
      expect(next.runs).toHaveLength(1);
    });
  });
});

// Ensure emptySession utility doesn't throw
describe('emptySession helper', () => {
  it('returns idle phase', () => {
    const s = emptySession();
    expect(s.phase).toBe('idle');
  });
});
