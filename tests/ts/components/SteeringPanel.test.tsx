/**
 * Tests for SteeringPanel component and applyDiff pure helper.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import '@testing-library/jest-dom';
import SteeringPanel, { applyDiff } from '../../../src/pages/Council/components/SteeringPanel';
import type { TaskGraph, GraphDiff, TaskNode } from '../../../src/types/council';

// ─── Fixtures ────────────────────────────────────────────────────────────────

const nodeA: TaskNode = {
  id: 'a',
  goal: 'goal A',
  depends_on: [],
  tool_allowlist: [],
  status: 'pending',
};

const nodeB: TaskNode = {
  id: 'b',
  goal: 'goal B',
  depends_on: ['a'],
  tool_allowlist: [],
  status: 'pending',
};

const baseGraph: TaskGraph = {
  goal: 'test goal',
  nodes: { a: nodeA, b: nodeB },
  hitl_mode: 'None',
};

// ─── applyDiff: AddNode ───────────────────────────────────────────────────────

describe('applyDiff — add_node', () => {
  it('adds the node to the graph', () => {
    const newNode: TaskNode = { id: 'c', goal: 'goal C', depends_on: ['b'], tool_allowlist: [], status: 'pending' };
    const diff: GraphDiff = { op: 'add_node', node: newNode };
    const result = applyDiff(baseGraph, diff);
    expect(result.nodes['c']).toEqual(newNode);
    expect(Object.keys(result.nodes)).toHaveLength(3);
  });
});

// ─── applyDiff: RemoveNode ────────────────────────────────────────────────────

describe('applyDiff — remove_node', () => {
  it('removes the node and strips outgoing edges', () => {
    const diff: GraphDiff = { op: 'remove_node', id: 'a' };
    const result = applyDiff(baseGraph, diff);
    expect(result.nodes['a']).toBeUndefined();
    expect(result.nodes['b'].depends_on).not.toContain('a');
  });
});

// ─── applyDiff: SplitNode ─────────────────────────────────────────────────────

describe('applyDiff — split_node', () => {
  it('removes original and repoints dependants', () => {
    const a1: TaskNode = { id: 'a1', goal: 'a1', depends_on: [], tool_allowlist: [], status: 'pending' };
    const a2: TaskNode = { id: 'a2', goal: 'a2', depends_on: [], tool_allowlist: [], status: 'pending' };
    const diff: GraphDiff = { op: 'split_node', id: 'a', into: [a1, a2] };
    const result = applyDiff(baseGraph, diff);
    expect(result.nodes['a']).toBeUndefined();
    expect(result.nodes['a1']).toBeDefined();
    expect(result.nodes['a2']).toBeDefined();
    expect(result.nodes['b'].depends_on).toContain('a1');
    expect(result.nodes['b'].depends_on).toContain('a2');
  });
});

// ─── applyDiff: RerouteEdge ───────────────────────────────────────────────────

describe('applyDiff — reroute_edge', () => {
  it('replaces old dep with new dep in the target node', () => {
    const nodeC: TaskNode = { id: 'c', goal: 'c', depends_on: [], tool_allowlist: [], status: 'pending' };
    const g: TaskGraph = { ...baseGraph, nodes: { ...baseGraph.nodes, c: nodeC } };
    const diff: GraphDiff = { op: 'reroute_edge', node_id: 'b', old_dep: 'a', new_dep: 'c' };
    const result = applyDiff(g, diff);
    expect(result.nodes['b'].depends_on).not.toContain('a');
    expect(result.nodes['b'].depends_on).toContain('c');
  });
});

// ─── applyDiff: SetTools ──────────────────────────────────────────────────────

describe('applyDiff — set_tools', () => {
  it('replaces the tool allowlist', () => {
    const diff: GraphDiff = { op: 'set_tools', id: 'a', tool_allowlist: ['search', 'calc'] };
    const result = applyDiff(baseGraph, diff);
    expect(result.nodes['a'].tool_allowlist).toEqual(['search', 'calc']);
  });
});

// ─── applyDiff: WrapInTeam ────────────────────────────────────────────────────

describe('applyDiff — wrap_in_team', () => {
  it('removes wrapped nodes and adds team node', () => {
    const diff: GraphDiff = { op: 'wrap_in_team', ids: ['a', 'b'], team_id: 'team_ab', team_goal: 'combined' };
    const result = applyDiff(baseGraph, diff);
    expect(result.nodes['a']).toBeUndefined();
    expect(result.nodes['b']).toBeUndefined();
    expect(result.nodes['team_ab']).toBeDefined();
    expect(result.nodes['team_ab'].goal).toBe('combined');
  });
});

// ─── SteeringPanel: renders input and submit button ──────────────────────────

describe('SteeringPanel rendering', () => {
  const onGraphChange = vi.fn();

  beforeEach(() => {
    vi.resetAllMocks();
    vi.stubGlobal('fetch', vi.fn());
  });

  it('renders the instruction textarea', () => {
    render(
      <SteeringPanel graph={baseGraph} port={9887} onGraphChange={onGraphChange} />
    );
    expect(screen.getByRole('textbox')).toBeInTheDocument();
  });

  it('submit button is disabled when input is empty', () => {
    render(
      <SteeringPanel graph={baseGraph} port={9887} onGraphChange={onGraphChange} />
    );
    const button = screen.getByRole('button', { name: /preview/i });
    expect(button).toBeDisabled();
  });

  it('shows diff preview after successful fetch', async () => {
    const diff: GraphDiff = { op: 'add_node', node: { id: 'c', goal: 'goal C', depends_on: [], tool_allowlist: [], status: 'pending' } };
    (fetch as ReturnType<typeof vi.fn>).mockResolvedValueOnce({
      ok: true,
      json: async () => ({ diff }),
    });

    render(
      <SteeringPanel graph={baseGraph} port={9887} onGraphChange={onGraphChange} />
    );

    fireEvent.change(screen.getByRole('textbox'), { target: { value: 'add a node c' } });
    fireEvent.click(screen.getByRole('button', { name: /preview/i }));

    await waitFor(() => {
      expect(screen.getByText(/ADD/i)).toBeInTheDocument();
    });
  });

  it('apply diff button calls onGraphChange', async () => {
    const diff: GraphDiff = { op: 'remove_node', id: 'b' };
    (fetch as ReturnType<typeof vi.fn>).mockResolvedValueOnce({
      ok: true,
      json: async () => ({ diff }),
    });

    render(
      <SteeringPanel graph={baseGraph} port={9887} onGraphChange={onGraphChange} />
    );

    fireEvent.change(screen.getByRole('textbox'), { target: { value: 'remove b' } });
    fireEvent.click(screen.getByRole('button', { name: /preview/i }));

    await waitFor(() => screen.getByText(/REMOVE/i));

    fireEvent.click(screen.getByRole('button', { name: /apply diff/i }));
    expect(onGraphChange).toHaveBeenCalledOnce();
    const newGraph = onGraphChange.mock.calls[0][0] as TaskGraph;
    expect(newGraph.nodes['b']).toBeUndefined();
  });
});
