import { describe, it, expect } from 'vitest';
import { ToolRegistry } from '../../../../src/services/tools/registry';

describe('ToolRegistry (secure-by-default enablement)', () => {
  it('defaults newly registered tools to disabled', () => {
    const registry = new ToolRegistry();

    registry.registerFunction(
      'test_tool',
      'A test tool',
      undefined,
      () => ({ success: true, data: 'ok' })
    );

    expect(registry.isEnabled('test_tool')).toBe(false);
    expect(registry.getEnabledDefinitions()).toHaveLength(0);
  });

  it('enable()/disable() toggles state as expected', () => {
    const registry = new ToolRegistry();

    registry.registerFunction(
      'test_tool',
      'A test tool',
      undefined,
      () => ({ success: true, data: 'ok' })
    );

    registry.enable('test_tool');
    expect(registry.isEnabled('test_tool')).toBe(true);
    expect(registry.getEnabledDefinitions().map((d) => d.function.name)).toEqual(['test_tool']);

    registry.disable('test_tool');
    expect(registry.isEnabled('test_tool')).toBe(false);
    expect(registry.getEnabledDefinitions()).toHaveLength(0);
  });

  it('unregister + re-register preserves previously enabled state (MCP resync simulation)', () => {
    const registry = new ToolRegistry();

    registry.registerFunction(
      'mcp_tool',
      'An MCP-provided tool',
      undefined,
      () => ({ success: true, data: 'ok' }),
      'mcp:server-1'
    );

    // User enables it
    registry.enable('mcp_tool');
    expect(registry.isEnabled('mcp_tool')).toBe(true);

    // Tool disappears during a resync (unregister), then comes back (register)
    expect(registry.unregister('mcp_tool')).toBe(true);

    registry.registerFunction(
      'mcp_tool',
      'An MCP-provided tool (re-registered)',
      undefined,
      () => ({ success: true, data: 'ok' }),
      'mcp:server-1'
    );

    // Should still be enabled after re-register
    expect(registry.isEnabled('mcp_tool')).toBe(true);
  });

  it('unregister + re-register keeps disabled tools disabled if never enabled', () => {
    const registry = new ToolRegistry();

    registry.registerFunction(
      'mcp_tool',
      'An MCP-provided tool',
      undefined,
      () => ({ success: true, data: 'ok' }),
      'mcp:server-1'
    );

    expect(registry.isEnabled('mcp_tool')).toBe(false);

    expect(registry.unregister('mcp_tool')).toBe(true);

    registry.registerFunction(
      'mcp_tool',
      'An MCP-provided tool (re-registered)',
      undefined,
      () => ({ success: true, data: 'ok' }),
      'mcp:server-1'
    );

    expect(registry.isEnabled('mcp_tool')).toBe(false);
  });
});
