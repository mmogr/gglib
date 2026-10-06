/**
 * What syncing the daemon's built-in tools leaves in the registry.
 *
 * The daemon runs a built-in tool; the registry holds what the UI needs for
 * one: its definition, the name a run's tool filter gives the daemon for it
 * (`builtin:<name>`), and the renderer that draws its result.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { syncBuiltinTools } from '../../../../src/services/tools/builtinIntegration';
import { resetToolRegistry, getToolRegistry } from '../../../../src/services/tools/registry';
import { timeRenderer } from '../../../../src/services/tools/renderers/TimeRenderer';
import type { McpTool } from '../../../../src/services/transport';

const transport = vi.hoisted(() => ({
  listBuiltinTools: vi.fn(),
}));

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

  it('registers each tool the daemon lists under the builtin source, and counts them', async () => {
    transport.listBuiltinTools.mockResolvedValue([makeTool('get_current_time'), makeTool('read_file')]);

    expect(await syncBuiltinTools()).toEqual({ added: 2, removed: 0 });

    const registry = getToolRegistry();
    expect(registry.getDefinitions().map((d) => d.function.name)).toEqual(['get_current_time', 'read_file']);
    expect(registry.getSource('get_current_time')).toBe('builtin');
    expect(registry.getSource('read_file')).toBe('builtin');
  });

  it('names a built-in tool to the daemon as `builtin:<name>`', async () => {
    transport.listBuiltinTools.mockResolvedValue([makeTool('get_current_time')]);

    await syncBuiltinTools();

    expect(getToolRegistry().getBackendName('get_current_time')).toBe('builtin:get_current_time');
  });

  it('gives get_current_time its renderer, and a tool with none of its own no renderer', async () => {
    transport.listBuiltinTools.mockResolvedValue([makeTool('get_current_time'), makeTool('read_file')]);

    await syncBuiltinTools();

    const registry = getToolRegistry();
    expect(registry.getRenderer('get_current_time')).toBe(timeRenderer);
    // No renderer registered is what sends a result to the fallback JSON view.
    expect(registry.getRenderer('read_file')).toBeUndefined();
  });

  it('replaces the built-ins of an earlier sync', async () => {
    transport.listBuiltinTools.mockResolvedValueOnce([makeTool('get_current_time'), makeTool('read_file')]);
    await syncBuiltinTools();
    transport.listBuiltinTools.mockResolvedValueOnce([makeTool('get_current_time')]);

    expect(await syncBuiltinTools()).toEqual({ added: 1, removed: 2 });

    expect(getToolRegistry().has('read_file')).toBe(false);
  });
});
