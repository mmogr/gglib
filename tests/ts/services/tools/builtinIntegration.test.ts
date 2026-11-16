/**
 * What syncing the daemon's built-in tools leaves in the registry.
 *
 * The daemon runs a built-in tool; the registry holds what the UI needs for
 * one: its definition, the name a run's tool filter gives the daemon for it
 * (`builtin:<name>`), and the renderer that draws its result. The list is
 * read before anything is replaced, so a read that fails leaves the tools
 * that were there; and the read is kept current, again each time the event
 * stream opens.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { keepBuiltinToolsCurrent, syncBuiltinTools } from '../../../../src/services/tools/builtinIntegration';
import { resetToolRegistry, getToolRegistry } from '../../../../src/services/tools/registry';
import { timeRenderer } from '../../../../src/services/tools/renderers/TimeRenderer';
import type { McpTool } from '../../../../src/services/transport';

const transport = vi.hoisted(() => {
  const opened = new Set<() => void>();
  return {
    listBuiltinTools: vi.fn(),
    opened,
    onEventStreamOpen: (listener: () => void) => {
      opened.add(listener);
      return () => void opened.delete(listener);
    },
    onEventStreamClose: vi.fn(() => () => {}),
    isEventStreamOpen: vi.fn(() => false),
  };
});

vi.mock('../../../../src/services/transport', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../../../src/services/transport')>()),
  getTransport: () => transport,
}));

vi.mock('../../../../src/services/platform', () => ({
  appLogger: { warn: vi.fn(), error: vi.fn(), info: vi.fn() },
}));

function makeTool(name: string): McpTool {
  return { name, description: `Description for ${name}`, input_schema: { type: 'object', properties: {} } };
}

describe('syncBuiltinTools', () => {
  beforeEach(() => {
    resetToolRegistry();
    transport.listBuiltinTools.mockReset();
  });

  it('registers each tool the daemon lists under the builtin source', async () => {
    transport.listBuiltinTools.mockResolvedValue([makeTool('get_current_time'), makeTool('other_tool')]);

    expect(await syncBuiltinTools()).toBe('applied');

    const registry = getToolRegistry();
    expect(registry.getDefinitions().map((d) => d.function.name)).toEqual(['get_current_time', 'other_tool']);
    expect(registry.getSource('get_current_time')).toBe('builtin');
    expect(registry.getSource('other_tool')).toBe('builtin');
  });

  it('names a built-in tool to the daemon as `builtin:<name>`', async () => {
    transport.listBuiltinTools.mockResolvedValue([makeTool('get_current_time')]);

    await syncBuiltinTools();

    expect(getToolRegistry().getBackendName('get_current_time')).toBe('builtin:get_current_time');
  });

  it('gives get_current_time its renderer, and a tool with none of its own no renderer', async () => {
    transport.listBuiltinTools.mockResolvedValue([makeTool('get_current_time'), makeTool('other_tool')]);

    await syncBuiltinTools();

    const registry = getToolRegistry();
    expect(registry.getRenderer('get_current_time')).toBe(timeRenderer);
    // No renderer registered is what sends a result to the fallback JSON view.
    expect(registry.getRenderer('other_tool')).toBeUndefined();
  });

  it('replaces the built-ins of an earlier sync', async () => {
    transport.listBuiltinTools.mockResolvedValueOnce([makeTool('get_current_time'), makeTool('other_tool')]);
    await syncBuiltinTools();
    transport.listBuiltinTools.mockResolvedValueOnce([makeTool('get_current_time')]);

    expect(await syncBuiltinTools()).toBe('applied');

    expect(getToolRegistry().has('get_current_time')).toBe(true);
    expect(getToolRegistry().has('other_tool')).toBe(false);
  });

  it('leaves the built-ins standing when a read fails', async () => {
    transport.listBuiltinTools.mockResolvedValueOnce([makeTool('get_current_time')]);
    await syncBuiltinTools();
    transport.listBuiltinTools.mockRejectedValueOnce(new TypeError('Failed to fetch'));

    expect(await syncBuiltinTools()).toBe('failed');

    expect(getToolRegistry().has('get_current_time')).toBe(true);
  });

  it('reads now and again each time the stream opens, until stopped', async () => {
    transport.listBuiltinTools.mockRejectedValueOnce(new TypeError('Failed to fetch'));
    const stop = keepBuiltinToolsCurrent();
    await vi.waitFor(() => expect(transport.listBuiltinTools).toHaveBeenCalledTimes(1));
    expect(getToolRegistry().has('get_current_time')).toBe(false);

    transport.listBuiltinTools.mockResolvedValueOnce([makeTool('get_current_time')]);
    transport.opened.forEach((listener) => listener());
    await vi.waitFor(() => expect(getToolRegistry().has('get_current_time')).toBe(true));

    stop();
    expect(transport.opened.size).toBe(0);
  });
});
