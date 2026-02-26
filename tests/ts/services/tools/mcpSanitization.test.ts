/**
 * Round-trip tests for MCP tool name sanitization.
 *
 * Covers the full lifecycle:
 *   register (sanitized name) → LLM ToolCall (sanitized name) → executeRawCall
 *   → executor closure (original name) → callMcpTool(serverId, originalName)
 *
 * These tests prove that the Phase-1 executor closure is the sole mechanism
 * responsible for translating sanitized names back to original MCP names, and
 * that the registry itself remains agnostic about MCP tool naming.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { registerMcpTools } from '../../../../src/services/tools/mcpIntegration';
import { resetToolRegistry, getToolRegistry } from '../../../../src/services/tools/registry';
import type { McpTool } from '../../../../src/services/clients/mcp';
import type { ToolCall } from '../../../../src/services/tools/types';

// =============================================================================
// Module mocks (same pattern as mcpIntegration.test.ts)
// =============================================================================

vi.mock('../../../../src/services/clients/mcp', () => ({
  listMcpServers: vi.fn(),
  callMcpTool: vi.fn(),
  isServerRunning: vi.fn(),
}));

const { mockWarn } = vi.hoisted(() => ({ mockWarn: vi.fn() }));

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

/**
 * Build a ToolCall object as the LLM would produce it — the name is always
 * the sanitized registry key because that is what was sent in the tool definitions.
 */
function makeToolCall(sanitizedName: string, args: Record<string, unknown> = {}, id = 'call-1'): ToolCall {
  return {
    id,
    type: 'function',
    function: {
      name: sanitizedName,
      arguments: JSON.stringify(args),
    },
  };
}

// =============================================================================
// Tests
// =============================================================================

describe('MCP tool name sanitization round-trip', () => {
  let callMcpTool: ReturnType<typeof vi.fn>;

  beforeEach(async () => {
    resetToolRegistry();
    mockWarn.mockClear();
    const mcp = await import('../../../../src/services/clients/mcp');
    callMcpTool = vi.mocked(mcp.callMcpTool);
    callMcpTool.mockReset();
  });

  // ── Core closure proof ─────────────────────────────────────────────────────

  it('calls MCP server with the exact original name, not the sanitized key', async () => {
    // Tool 'get-weather.v2' from server 'test' sanitizes to 'mcp_test_get-weather_v2'
    callMcpTool.mockResolvedValueOnce({ success: true, data: 'sunny' });

    registerMcpTools('test', [makeTool('get-weather.v2')]);

    const toolCall = makeToolCall('mcp_test_get-weather_v2', { city: 'London' });
    await getToolRegistry().executeRawCall(toolCall);

    // Strict assertion: the MCP server must receive the raw original name
    expect(callMcpTool).toHaveBeenCalledOnce();
    expect(callMcpTool).toHaveBeenCalledWith(
      'test',           // serverId — unchanged
      'get-weather.v2', // raw original name — NOT 'mcp_test_get-weather_v2'
      { city: 'London' },
    );
  });

  it('passes arguments through unmodified', async () => {
    callMcpTool.mockResolvedValueOnce({ success: true, data: null });

    registerMcpTools('srv', [makeTool('echo')]);
    const args = { message: 'hello', count: 3, flag: true };

    await getToolRegistry().executeRawCall(makeToolCall('mcp_srv_echo', args));

    expect(callMcpTool).toHaveBeenCalledWith('srv', 'echo', args);
  });

  // ── Success / error forwarding ─────────────────────────────────────────────

  it('returns a success result with data from the MCP server', async () => {
    callMcpTool.mockResolvedValueOnce({ success: true, data: { temp: 20, unit: 'C' } });

    registerMcpTools('weather', [makeTool('get-temp')]);
    const result = await getToolRegistry().executeRawCall(
      makeToolCall('mcp_weather_get-temp', { city: 'Paris' }),
    );

    expect(result).toEqual({ success: true, data: { temp: 20, unit: 'C' } });
  });

  it('returns an error result when the MCP server responds with failure', async () => {
    callMcpTool.mockResolvedValueOnce({ success: false, error: 'city not found' });

    registerMcpTools('weather', [makeTool('get-temp')]);
    const result = await getToolRegistry().executeRawCall(
      makeToolCall('mcp_weather_get-temp', { city: 'Atlantis' }),
    );

    expect(result).toEqual({ success: false, error: 'city not found' });
  });

  it('falls back to a generic error message when MCP responds with success:false but no error string', async () => {
    callMcpTool.mockResolvedValueOnce({ success: false });

    registerMcpTools('srv', [makeTool('flaky')]);
    const result = await getToolRegistry().executeRawCall(makeToolCall('mcp_srv_flaky'));

    expect(result).toMatchObject({ success: false });
    expect((result as { success: false; error: string }).error).toBeTruthy();
  });

  // ── Rejection / network failure ────────────────────────────────────────────

  it('returns an error result when callMcpTool rejects (does not throw)', async () => {
    callMcpTool.mockRejectedValueOnce(new Error('network failure'));

    registerMcpTools('srv', [makeTool('risky-op')]);
    const result = await getToolRegistry().executeRawCall(makeToolCall('mcp_srv_risky-op'));

    expect(result).toMatchObject({
      success: false,
      error: 'MCP call failed: network failure',
    });
  });

  it('handles non-Error rejections gracefully', async () => {
    callMcpTool.mockRejectedValueOnce('something broke');

    registerMcpTools('srv', [makeTool('tool_a')]);
    const result = await getToolRegistry().executeRawCall(makeToolCall('mcp_srv_tool_a'));

    expect(result).toMatchObject({ success: false });
    expect((result as { success: false; error: string }).error).toContain('MCP call failed');
  });

  // ── Argument JSON parsing ──────────────────────────────────────────────────

  it('returns a parse-error result when the LLM sends invalid arguments JSON', async () => {
    registerMcpTools('srv', [makeTool('my_tool')]);

    const badCall: ToolCall = {
      id: 'call-bad',
      type: 'function',
      function: { name: 'mcp_srv_my_tool', arguments: 'not-json{{' },
    };
    const result = await getToolRegistry().executeRawCall(badCall);

    expect(result).toMatchObject({ success: false });
    expect((result as { success: false; error: string }).error).toContain('Failed to parse');
    // The MCP server must NOT have been contacted
    expect(callMcpTool).not.toHaveBeenCalled();
  });

  // ── Unknown / unregistered tool name ──────────────────────────────────────

  it('returns an unknown-tool error for a sanitized name that was never registered', async () => {
    const result = await getToolRegistry().executeRawCall(
      makeToolCall('mcp_ghost_server_nonexistent'),
    );

    expect(result).toMatchObject({
      success: false,
      error: expect.stringContaining('Unknown tool'),
    });
    expect(callMcpTool).not.toHaveBeenCalled();
  });

  // ── Multiple tools from the same server ───────────────────────────────────

  it('routes multiple tools from the same server to their respective original names', async () => {
    callMcpTool
      .mockResolvedValueOnce({ success: true, data: 'weather-result' })
      .mockResolvedValueOnce({ success: true, data: 'files-result' });

    registerMcpTools('multi', [
      makeTool('get-weather.v2'),
      makeTool('list files'),
    ]);

    await getToolRegistry().executeRawCall(makeToolCall('mcp_multi_get-weather_v2', {}, 'call-1'));
    await getToolRegistry().executeRawCall(makeToolCall('mcp_multi_list_files', {}, 'call-2'));

    expect(callMcpTool).toHaveBeenNthCalledWith(1, 'multi', 'get-weather.v2', {});
    expect(callMcpTool).toHaveBeenNthCalledWith(2, 'multi', 'list files', {});
  });

  // ── Two servers, same tool name (namespace isolation) ─────────────────────

  it('routes tools with the same name from different servers to the correct server', async () => {
    callMcpTool
      .mockResolvedValueOnce({ success: true, data: 'from-alpha' })
      .mockResolvedValueOnce({ success: true, data: 'from-beta' });

    registerMcpTools('alpha', [makeTool('ping')]);
    registerMcpTools('beta',  [makeTool('ping')]);

    await getToolRegistry().executeRawCall(makeToolCall('mcp_alpha_ping', {}, 'c1'));
    await getToolRegistry().executeRawCall(makeToolCall('mcp_beta_ping',  {}, 'c2'));

    expect(callMcpTool).toHaveBeenNthCalledWith(1, 'alpha', 'ping', {});
    expect(callMcpTool).toHaveBeenNthCalledWith(2, 'beta',  'ping', {});
  });

  // ── Collision: skipped tools must not be callable ─────────────────────────

  it('does not register either tool when two names collide, so calling the sanitized key returns an error', async () => {
    // 'get!data' and 'get?data' both sanitize to 'mcp_s_get_data' — both are skipped
    registerMcpTools('s', [makeTool('get!data'), makeTool('get?data')]);

    const result = await getToolRegistry().executeRawCall(makeToolCall('mcp_s_get_data'));

    expect(result).toMatchObject({
      success: false,
      error: expect.stringContaining('Unknown tool'),
    });
    expect(callMcpTool).not.toHaveBeenCalled();
  });

  // ── Clean name requires no sanitization ───────────────────────────────────

  it('works correctly when the tool name needs no sanitization at all', async () => {
    callMcpTool.mockResolvedValueOnce({ success: true, data: 42 });

    registerMcpTools('srv', [makeTool('get_current_time')]);
    const result = await getToolRegistry().executeRawCall(
      makeToolCall('mcp_srv_get_current_time'),
    );

    expect(result).toEqual({ success: true, data: 42 });
    expect(callMcpTool).toHaveBeenCalledWith('srv', 'get_current_time', {});
  });
});
