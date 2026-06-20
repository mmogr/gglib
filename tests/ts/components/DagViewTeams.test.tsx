/**
 * DagView tests — collapsible team node expand/collapse.
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import DagView from '../../../src/pages/Council/components/DagView';
import type { TaskGraph } from '../../../src/types/council';

// ─── Helpers ──────────────────────────────────────────────────────────────────

function leafGraph(): TaskGraph {
  return {
    nodes: {
      a: { id: 'a', goal: 'Task A', depends_on: [], tool_allowlist: [], status: 'pending' },
      b: { id: 'b', goal: 'Task B', depends_on: ['a'], tool_allowlist: [], status: 'pending' },
    },
  };
}

function teamGraph(): TaskGraph {
  const subgraph: TaskGraph = {
    nodes: {
      sub1: {
        id: 'sub1',
        goal: 'Sub-task 1',
        depends_on: [],
        tool_allowlist: [],
        status: 'pending',
      },
      sub2: {
        id: 'sub2',
        goal: 'Sub-task 2',
        depends_on: ['sub1'],
        tool_allowlist: [],
        status: 'pending',
      },
    },
  };
  return {
    nodes: {
      team_alpha: {
        id: 'team_alpha',
        goal: 'Alpha team goal',
        depends_on: [],
        tool_allowlist: [],
        status: 'pending',
        kind: { team: { subgraph } },
      },
      leaf_after: {
        id: 'leaf_after',
        goal: 'Post-team task',
        depends_on: ['team_alpha'],
        tool_allowlist: [],
        status: 'pending',
      },
    },
  };
}

// ─── Basic leaf rendering ─────────────────────────────────────────────────────

describe('DagView - leaf nodes', () => {
  it('renders all leaf nodes', () => {
    render(<DagView graph={leafGraph()} nodeStates={{}} />);
    expect(screen.getByTestId('dag-node-a')).toBeInTheDocument();
    expect(screen.getByTestId('dag-node-b')).toBeInTheDocument();
  });

  it('marks selected node with aria-pressed=true', () => {
    render(<DagView graph={leafGraph()} nodeStates={{}} selectedNodeId="a" />);
    expect(screen.getByTestId('dag-node-a')).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByTestId('dag-node-b')).toHaveAttribute('aria-pressed', 'false');
  });

  it('calls onSelectNode when a leaf is clicked', () => {
    const onSelect = vi.fn();
    render(<DagView graph={leafGraph()} nodeStates={{}} onSelectNode={onSelect} />);
    fireEvent.click(screen.getByTestId('dag-node-a'));
    expect(onSelect).toHaveBeenCalledWith('a');
  });
});

// ─── Team node collapse / expand ──────────────────────────────────────────────

describe('DagView - team node collapse/expand', () => {
  beforeEach(() => {
    sessionStorage.clear();
  });
  afterEach(() => {
    sessionStorage.clear();
  });

  it('renders team header with aria-expanded=false initially', () => {
    render(<DagView graph={teamGraph()} nodeStates={{}} />);
    const header = screen.getByTestId('team-header-team_alpha');
    expect(header).toBeInTheDocument();
    expect(header).toHaveAttribute('aria-expanded', 'false');
  });

  it('does NOT show subgraph nodes when collapsed', () => {
    render(<DagView graph={teamGraph()} nodeStates={{}} />);
    expect(screen.queryByTestId('dag-node-sub1')).not.toBeInTheDocument();
    expect(screen.queryByTestId('dag-node-sub2')).not.toBeInTheDocument();
  });

  it('reveals subgraph nodes after clicking team header', () => {
    render(<DagView graph={teamGraph()} nodeStates={{}} />);
    const header = screen.getByTestId('team-header-team_alpha');
    fireEvent.click(header);
    expect(header).toHaveAttribute('aria-expanded', 'true');
    expect(screen.getByTestId('dag-node-sub1')).toBeInTheDocument();
    expect(screen.getByTestId('dag-node-sub2')).toBeInTheDocument();
  });

  it('collapses again on second click', () => {
    render(<DagView graph={teamGraph()} nodeStates={{}} />);
    const header = screen.getByTestId('team-header-team_alpha');
    fireEvent.click(header);
    fireEvent.click(header);
    expect(header).toHaveAttribute('aria-expanded', 'false');
    expect(screen.queryByTestId('dag-node-sub1')).not.toBeInTheDocument();
  });

  it('persists expanded state in sessionStorage when runId is provided', () => {
    render(<DagView graph={teamGraph()} nodeStates={{}} runId="run-42" />);
    const header = screen.getByTestId('team-header-team_alpha');
    fireEvent.click(header);
    const stored = JSON.parse(sessionStorage.getItem('orch_dag_expanded_run-42') ?? '[]') as string[];
    expect(stored).toContain('team_alpha');
  });

  it('renders non-team siblings alongside the team header', () => {
    render(<DagView graph={teamGraph()} nodeStates={{}} />);
    expect(screen.getByTestId('dag-node-leaf_after')).toBeInTheDocument();
  });
});
