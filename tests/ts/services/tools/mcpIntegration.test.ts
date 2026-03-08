import { describe, it, expect, vi, beforeEach } from 'vitest';
import { registerMcpTools, unregisterMcpTools, getMcpSource } from '../../../../src/services/tools/mcpIntegration';
import { resetToolRegistry, getToolRegistry } from '../../../../src/services/tools/registry';
import { mcpGenericRenderer } from '../../../../src/services/tools/renderers';
import type { McpTool } from '../../../../src/services/clients/mcp';

// =============================================================================
// Module mocks
// =============================================================================

// Mock the MCP client so we never make real Tauri IPC calls in unit tests.
vi.mock('../../../../src/services/clients/mcp', () => ({
  listMcpServers: vi.fn(),
  callMcpTool: vi.fn(),
  isServerRunning: vi.fn(),
}));

// vi.mock factories are hoisted to the top of the file, so any variables they
// reference must also be hoisted via vi.hoisted() to avoid TDZ errors.
const { mockWarn } = vi.hoisted(() => ({ mockWarn: vi.fn() }));

// Mock appLogger to prevent console noise and allow assertion on warn calls.
vi.mock('../../../../src/services/platform', () => ({
  appLogger: {
    warn: mockWarn,
    error: vi.fn(),
    info: vi.fn(),
  },
}));

// =============================================================================
// Helpers
// =============================================================================

function makeTool(name: string, description = `Description for ${name}`): McpTool {
  return {
    name,
    description,
    input_schema: { type: 'object', properties: {} },
  };
}

// =============================================================================
// Tests
// =============================================================================

describe('registerMcpTools', () => {
  beforeEach(() => {
    resetToolRegistry();
    mockWarn.mockClear();
  });

  // ── Basic registration ─────────────────────────────────────────────────────

  it('registers tools with sanitized names and returns count', () => {
    const count = registerMcpTools('my-server', [
      makeTool('get-weather'),
      makeTool('list_files'),
    ]);
    expect(count).toBe(2);
  });

  it('registers tools under the correct source', () => {
    registerMcpTools('srv1', [makeTool('get_time')]);
    const registry = getToolRegistry();
    expect(registry.getSource('mcp_srv1_get_time')).toBe('mcp:srv1');
  });

  it('stores the sanitized name as the registry key', () => {
    registerMcpTools('my.server', [makeTool('get data!')]);
    const registry = getToolRegistry();
    // 'mcp_my.server_get data!' sanitizes to 'mcp_my_server_get_data'
    expect(registry.has('mcp_my_server_get_data')).toBe(true);
    expect(registry.has('mcp_my.server_get data!')).toBe(false);
  });

  // ── Reverse name map ───────────────────────────────────────────────────────

  it('getOriginalName returns the raw MCP tool name for a sanitized key', () => {
    registerMcpTools('my.server', [makeTool('get data!')]);
    const registry = getToolRegistry();
    expect(registry.getOriginalName('mcp_my_server_get_data')).toBe('get data!');
  });

  it('getServerId returns the server ID for a sanitized key', () => {
    registerMcpTools('my.server', [makeTool('get data!')]);
    const registry = getToolRegistry();
    expect(registry.getServerId('mcp_my_server_get_data')).toBe('my.server');
  });

  it('lookup returns undefined for non-MCP built-in tool names', () => {
    const registry = getToolRegistry();
    expect(registry.getOriginalName('get_current_time')).toBeUndefined();
  });

  it('lookup works for clean names with no sanitization needed', () => {
    registerMcpTools('srv1', [makeTool('list_files')]);
    const registry = getToolRegistry();
    expect(registry.getOriginalName('mcp_srv1_list_files')).toBe('list_files');
    expect(registry.getServerId('mcp_srv1_list_files')).toBe('srv1');
  });

  // ── Sanitization warnings ──────────────────────────────────────────────────

  it('logs a warn when sanitization changes the tool name', () => {
    registerMcpTools('my.server', [makeTool('get data!')]);
    expect(mockWarn).toHaveBeenCalledWith(
      'service.mcp',
      'MCP tool name was sanitized',
      expect.objectContaining({
        serverId: 'my.server',
        original: 'mcp_my.server_get data!',
        sanitized: 'mcp_my_server_get_data',
      }),
    );
  });

  it('does not warn when no sanitization is needed', () => {
    mockWarn.mockClear();
    registerMcpTools('srv1', [makeTool('get_weather')]);
    // mockWarn should not have been called with 'MCP tool name was sanitized'
    const sanitizeWarnings = mockWarn.mock.calls.filter(
      ([, msg]) => msg === 'MCP tool name was sanitized',
    );
    expect(sanitizeWarnings).toHaveLength(0);
  });

  // ── Collision detection ────────────────────────────────────────────────────

  it('skips both tools when two names collide and logs a warning', () => {
    // 'get!data' and 'get?data' both sanitize to 'mcp_s_get_data'
    const count = registerMcpTools('s', [makeTool('get!data'), makeTool('get?data')]);
    expect(count).toBe(0);
    const registry = getToolRegistry();
    expect(registry.has('mcp_s_get_data')).toBe(false);
    expect(mockWarn).toHaveBeenCalledWith(
      'service.mcp',
      'MCP tool name collision detected \u2014 skipping affected tools',
      expect.objectContaining({ serverId: 's' }),
    );
  });

  it('registers non-colliding tools even when some in the batch collide', () => {
    const count = registerMcpTools('s', [
      makeTool('get!data'),   // collides
      makeTool('get?data'),   // collides
      makeTool('list_files'), // safe
    ]);
    expect(count).toBe(1);
    const registry = getToolRegistry();
    expect(registry.has('mcp_s_list_files')).toBe(true);
  });

  // ── Executor uses raw tool name ────────────────────────────────────────────

  it('executor calls callMcpTool with the raw original tool name, not the sanitized name', async () => {
    const { callMcpTool } = await import('../../../../src/services/clients/mcp');
    const mockCallMcpTool = vi.mocked(callMcpTool);
    mockCallMcpTool.mockResolvedValueOnce({ success: true, data: 'result' });

    registerMcpTools('my.server', [makeTool('get data!')]);

    const registry = getToolRegistry();
    // Execute via the sanitized key
    await registry.execute('mcp_my_server_get_data', {});

    expect(mockCallMcpTool).toHaveBeenCalledWith(
      'my.server',
      'get data!',  // raw name, not 'mcp_my_server_get_data'
      {},
    );
  });

  // ── Renderer assignment ────────────────────────────────────────────────────

  it('assigns mcpGenericRenderer to tools without an output_schema', () => {
    registerMcpTools('srv1', [makeTool('get_weather')]);
    const registry = getToolRegistry();
    expect(registry.getRenderer('mcp_srv1_get_weather')).toBe(mcpGenericRenderer);
  });

  it('assigns a schema renderer (not mcpGenericRenderer) to tools with an output_schema', () => {
    const toolWithSchema: McpTool = {
      ...makeTool('search_results'),
      output_schema: { type: 'object', properties: { items: { type: 'array' } } },
    };
    registerMcpTools('srv1', [toolWithSchema]);
    const registry = getToolRegistry();
    const renderer = registry.getRenderer('mcp_srv1_search_results');
    expect(renderer).toBeDefined();
    expect(renderer).not.toBe(mcpGenericRenderer);
  });
});

// =============================================================================
// unregisterMcpTools
// =============================================================================

describe('unregisterMcpTools', () => {
  beforeEach(() => {
    resetToolRegistry();
    mockWarn.mockClear();
  });

  it('removes registered tools and returns the count', () => {
    registerMcpTools('srv1', [makeTool('get_weather'), makeTool('list_files')]);
    const removed = unregisterMcpTools('srv1');
    expect(removed).toBe(2);
    const registry = getToolRegistry();
    expect(registry.has('mcp_srv1_get_weather')).toBe(false);
    expect(registry.has('mcp_srv1_list_files')).toBe(false);
  });

  it('clears name-map entries for the unregistered server', () => {
    registerMcpTools('my.server', [makeTool('get data!')]);
    unregisterMcpTools('my.server');
    const registry = getToolRegistry();
    expect(registry.getOriginalName('mcp_my_server_get_data')).toBeUndefined();
    expect(registry.getServerId('mcp_my_server_get_data')).toBeUndefined();
  });

  it('does not remove tools from a different server', () => {
    registerMcpTools('srv1', [makeTool('tool_a')]);
    registerMcpTools('srv2', [makeTool('tool_b')]);
    unregisterMcpTools('srv1');
    const registry = getToolRegistry();
    expect(registry.has('mcp_srv2_tool_b')).toBe(true);
    expect(registry.getOriginalName('mcp_srv2_tool_b')).toBe('tool_b');
  });

  it('returns 0 when the server has no registered tools', () => {
    expect(unregisterMcpTools('nonexistent')).toBe(0);
  });
});

// =============================================================================
// getMcpSource
// =============================================================================

describe('getMcpSource', () => {
  it('returns the correct source prefix format', () => {
    expect(getMcpSource('my-server')).toBe('mcp:my-server');
    expect(getMcpSource('123')).toBe('mcp:123');
  });
});
