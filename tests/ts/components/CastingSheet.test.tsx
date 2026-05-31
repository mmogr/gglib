/**
 * CastingSheet tests — actor-card grid, role icons, leaf traversal.
 */

import { describe, it, expect } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import CastingSheet, { collectLeafNodes } from '../../../src/pages/Orchestrator/components/CastingSheet';
import type { TaskGraph } from '../../../src/types/orchestrator';

// ─── Fixtures ─────────────────────────────────────────────────────────────────

function makeGraph(overrides: Partial<TaskGraph['nodes']> = {}): TaskGraph {
  return { nodes: overrides };
}

const ROLES = [
  'researcher',
  'red-team',
  'fact-checker',
  'writer',
  'editor',
  'critic',
  'synthesizer',
] as const;

function allRolesGraph(): TaskGraph {
  const nodes: TaskGraph['nodes'] = {};
  for (const role of ROLES) {
    nodes[role] = {
      id: role,
      goal: `Goal for ${role}`,
      depends_on: [],
      tool_allowlist: [],
      status: 'pending',
      role,
    };
  }
  return { nodes };
}

// ─── collectLeafNodes unit tests ──────────────────────────────────────────────

describe('collectLeafNodes', () => {
  it('returns all nodes when none have team kind', () => {
    const graph = makeGraph({
      a: { id: 'a', goal: 'A', depends_on: [], tool_allowlist: [], status: 'pending' },
      b: { id: 'b', goal: 'B', depends_on: ['a'], tool_allowlist: [], status: 'pending' },
    });
    const cards = collectLeafNodes(graph);
    expect(cards.map((c) => c.nodeId)).toEqual(expect.arrayContaining(['a', 'b']));
    expect(cards).toHaveLength(2);
  });

  it('recurses into team subgraph and tags ancestorTeamId', () => {
    const subgraph: TaskGraph = {
      nodes: {
        child: {
          id: 'child',
          goal: 'child goal',
          depends_on: [],
          tool_allowlist: [],
          status: 'pending',
        },
      },
    };
    const graph = makeGraph({
      myteam: {
        id: 'myteam',
        goal: 'Team goal',
        depends_on: [],
        tool_allowlist: [],
        status: 'pending',
        kind: { team: { subgraph } },
      },
    });

    const cards = collectLeafNodes(graph);
    expect(cards).toHaveLength(1);
    expect(cards[0].nodeId).toBe('child');
    expect(cards[0].ancestorTeamId).toBe('myteam');
  });

  it('returns empty array for empty graph', () => {
    expect(collectLeafNodes({ nodes: {} })).toHaveLength(0);
  });
});

// ─── CastingSheet render tests ────────────────────────────────────────────────

describe('CastingSheet', () => {
  it('renders a card for each leaf node', () => {
    const graph = makeGraph({
      a: { id: 'a', goal: 'Do A', depends_on: [], tool_allowlist: [], status: 'pending' },
      b: { id: 'b', goal: 'Do B', depends_on: [], tool_allowlist: [], status: 'pending' },
    });
    render(<CastingSheet graph={graph} nodeStates={{}} />);
    expect(screen.getByTestId('casting-card-a')).toBeInTheDocument();
    expect(screen.getByTestId('casting-card-b')).toBeInTheDocument();
  });

  it('renders all 7 known role icons without error', () => {
    render(<CastingSheet graph={allRolesGraph()} nodeStates={{}} />);
    for (const role of ROLES) {
      expect(screen.getByTestId(`role-icon-${role}`)).toBeInTheDocument();
    }
  });

  it('renders "generic" icon for unknown role', () => {
    const graph = makeGraph({
      unknown: {
        id: 'unknown',
        goal: 'Mystery task',
        depends_on: [],
        tool_allowlist: [],
        status: 'pending',
        role: 'quantum-philosopher',
      },
    });
    render(<CastingSheet graph={graph} nodeStates={{}} />);
    // Falls back to generic (no role-icon-quantum-philosopher, uses fallback User icon)
    expect(screen.getByTestId('role-icon-quantum-philosopher')).toBeInTheDocument();
  });

  it('renders empty state when graph has no leaf nodes', () => {
    render(<CastingSheet graph={{ nodes: {} }} nodeStates={{}} />);
    expect(screen.getByTestId('casting-empty')).toBeInTheDocument();
  });

  it('marks selected card with aria-pressed=true', () => {
    const graph = makeGraph({
      a: { id: 'a', goal: 'Do A', depends_on: [], tool_allowlist: [], status: 'pending' },
    });
    render(<CastingSheet graph={graph} nodeStates={{}} selectedNodeId="a" />);
    const card = screen.getByTestId('casting-card-a');
    expect(card).toHaveAttribute('aria-pressed', 'true');
  });

  it('calls onSelectNode when a card is clicked', () => {
    const graph = makeGraph({
      a: { id: 'a', goal: 'Do A', depends_on: [], tool_allowlist: [], status: 'pending' },
    });
    const onSelect = vi.fn();
    render(<CastingSheet graph={graph} nodeStates={{}} onSelectNode={onSelect} />);
    fireEvent.click(screen.getByTestId('casting-card-a'));
    expect(onSelect).toHaveBeenCalledWith('a');
  });

  it('renders ancestor team label for nodes inside a team subgraph', () => {
    const subgraph: TaskGraph = {
      nodes: {
        child: {
          id: 'child',
          goal: 'child goal',
          depends_on: [],
          tool_allowlist: [],
          status: 'pending',
        },
      },
    };
    const graph = makeGraph({
      squad: {
        id: 'squad',
        goal: 'Squad goal',
        depends_on: [],
        tool_allowlist: [],
        status: 'pending',
        kind: { team: { subgraph } },
      },
    });
    render(<CastingSheet graph={graph} nodeStates={{}} />);
    expect(screen.getByText(/team: squad/)).toBeInTheDocument();
  });

  it('renders tool chips', () => {
    const graph = makeGraph({
      a: {
        id: 'a',
        goal: 'Do A',
        depends_on: [],
        tool_allowlist: ['web_search', 'python'],
        status: 'pending',
      },
    });
    render(<CastingSheet graph={graph} nodeStates={{}} />);
    expect(screen.getByText('web_search')).toBeInTheDocument();
    expect(screen.getByText('python')).toBeInTheDocument();
  });
});
