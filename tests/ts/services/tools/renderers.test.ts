import { describe, it, expect } from 'vitest';
import { ToolRegistry } from '../../../../src/services/tools/registry';
import type { ToolResultRenderer } from '../../../../src/services/tools/types';

// ---------------------------------------------------------------------------
// ToolRegistry.getRenderer
// ---------------------------------------------------------------------------

describe('ToolRegistry.getRenderer', () => {
  const mockRenderer: ToolResultRenderer = {
    renderResult: (data) => String(data),
  };

  it('returns the registered renderer for a tool registered with one', () => {
    const registry = new ToolRegistry();
    registry.registerFunction(
      'dummy',
      'A dummy tool',
      undefined,
      'builtin',
      mockRenderer,
    );
    expect(registry.getRenderer('dummy')).toBe(mockRenderer);
  });

  it('returns undefined for a tool registered without a renderer', () => {
    const registry = new ToolRegistry();
    registry.registerFunction(
      'no_renderer',
      'A tool without a renderer',
      undefined,
    );
    expect(registry.getRenderer('no_renderer')).toBeUndefined();
  });

  it('returns undefined for an unknown tool name', () => {
    const registry = new ToolRegistry();
    expect(registry.getRenderer('unknown_tool')).toBeUndefined();
  });
});
