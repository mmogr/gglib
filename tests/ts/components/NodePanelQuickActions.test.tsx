/**
 * NodePanel quick-action button tests.
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import NodePanel from '../../../src/pages/Council/components/NodePanel';
import type { TaskGraph } from '../../../src/types/council';

// ─── Fixtures ─────────────────────────────────────────────────────────────────

const graph: TaskGraph = {
  nodes: {
    researcher: {
      id: 'researcher',
      goal: 'Research the topic',
      depends_on: [],
      tool_allowlist: ['web_search'],
      status: 'pending',
      role: 'researcher',
    },
  },
};

const node = graph.nodes['researcher'];

// ─── Helpers ──────────────────────────────────────────────────────────────────

function mockFetchOk(body: unknown) {
  return vi.fn().mockResolvedValue({
    ok: true,
    json: async () => body,
    text: async () => JSON.stringify(body),
  });
}

function mockFetchError(status: number, text: string) {
  return vi.fn().mockResolvedValue({
    ok: false,
    status,
    text: async () => text,
    json: async () => ({}),
  });
}

// ─── Tests ────────────────────────────────────────────────────────────────────

describe('NodePanel quick actions', () => {
  let originalFetch: typeof globalThis.fetch;

  beforeEach(() => {
    originalFetch = globalThis.fetch;
  });

  afterEach(() => {
    globalThis.fetch = originalFetch;
    vi.restoreAllMocks();
  });

  it('renders all 4 quick-action buttons when graph prop is provided', () => {
    render(
      <NodePanel
        nodeId="researcher"
        node={node}
        nodeState={undefined}
        defaultOpen
        graph={graph}
        port={9000}
      />,
    );
    expect(screen.getByRole('button', { name: /Add critic for node researcher/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Split into 3 parallel for node researcher/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Wrap in team for node researcher/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Re-run with feedback for node researcher/i })).toBeInTheDocument();
  });

  it('does NOT render quick-action buttons when graph prop is absent', () => {
    render(
      <NodePanel
        nodeId="researcher"
        node={node}
        nodeState={undefined}
        defaultOpen
      />,
    );
    expect(screen.queryByRole('button', { name: /Add critic/i })).not.toBeInTheDocument();
  });

  it('calls steer endpoint on quick-action click (no runId)', async () => {
    const diff = { added: [], removed: [], modified: [] };
    globalThis.fetch = mockFetchOk({ diff });

    render(
      <NodePanel
        nodeId="researcher"
        node={node}
        nodeState={undefined}
        defaultOpen
        graph={graph}
        port={9000}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: /Add critic for node researcher/i }));

    await waitFor(() => {
      expect(globalThis.fetch).toHaveBeenCalledWith(
        '/api/council/steer',
        expect.objectContaining({ method: 'POST' }),
      );
    });

    const [, init] = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls[0] as [string, RequestInit];
    const body = JSON.parse(init.body as string) as { instruction: string; graph: TaskGraph; port: number };
    expect(body.instruction).toMatch(/Add a critic node/);
    expect(body.graph).toEqual(graph);
    expect(body.port).toBe(9000);
  });

  it('shows diff preview after successful steer call', async () => {
    const diff = { added: [{ id: 'critic', goal: 'Critique', depends_on: ['researcher'], tool_allowlist: [], status: 'pending' }], removed: [], modified: [] };
    globalThis.fetch = mockFetchOk({ diff });

    render(
      <NodePanel
        nodeId="researcher"
        node={node}
        nodeState={undefined}
        defaultOpen
        graph={graph}
        port={9000}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: /Add critic for node researcher/i }));

    await waitFor(() => {
      expect(screen.getByText('Proposed changes')).toBeInTheDocument();
    });
  });

  it('calls note endpoint when runId is provided', async () => {
    globalThis.fetch = mockFetchOk({});

    render(
      <NodePanel
        nodeId="researcher"
        node={node}
        nodeState={undefined}
        defaultOpen
        graph={graph}
        port={9000}
        runId="run-abc"
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: /Split into 3 parallel for node researcher/i }));

    await waitFor(() => {
      expect(globalThis.fetch).toHaveBeenCalledWith(
        '/api/council/runs/run-abc/note',
        expect.objectContaining({ method: 'POST' }),
      );
    });

    const [, init] = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls[0] as [string, RequestInit];
    const body = JSON.parse(init.body as string) as { instruction: string };
    expect(body.instruction).toMatch(/Split the "researcher" node/);
  });

  it('shows success confirmation after note is sent', async () => {
    globalThis.fetch = mockFetchOk({});

    render(
      <NodePanel
        nodeId="researcher"
        node={node}
        nodeState={undefined}
        defaultOpen
        graph={graph}
        port={9000}
        runId="run-xyz"
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: /Re-run with feedback for node researcher/i }));

    await waitFor(() => {
      expect(screen.getByText(/Steering note sent/i)).toBeInTheDocument();
    });
  });

  it('shows error message when steer call fails', async () => {
    globalThis.fetch = mockFetchError(500, 'Internal server error');

    render(
      <NodePanel
        nodeId="researcher"
        node={node}
        nodeState={undefined}
        defaultOpen
        graph={graph}
        port={9000}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: /Wrap in team for node researcher/i }));

    await waitFor(() => {
      expect(screen.getByText(/Error:/)).toBeInTheDocument();
    });
  });

  it('discard button resets diff preview', async () => {
    const diff = { added: [], removed: [], modified: [] };
    globalThis.fetch = mockFetchOk({ diff });

    render(
      <NodePanel
        nodeId="researcher"
        node={node}
        nodeState={undefined}
        defaultOpen
        graph={graph}
        port={9000}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: /Add critic for node researcher/i }));
    await waitFor(() => screen.getByText('Proposed changes'));

    fireEvent.click(screen.getByRole('button', { name: /Discard/i }));
    expect(screen.queryByText('Proposed changes')).not.toBeInTheDocument();
  });
});
