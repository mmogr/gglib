/**
 * Round-trip tests for MCP tool name sanitization.
 *
 * Covers the full lifecycle:
 *   register (sanitized name) → the composer's Tools ▸ names it to the
 *   daemon, in the chat's saved tools, as `serverId:originalName`
 *
 * The daemon runs the tool, and the MCP server only knows its own naming. So
 * what must survive sanitization is the server and the original name, read
 * back from the sanitized key the UI holds.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { registerMcpTools } from '../../../../src/services/tools/mcpIntegration';
import { resetToolRegistry, getToolRegistry } from '../../../../src/services/tools/registry';
import type { McpTool } from '../../../../src/services/transport';
import type { McpServerId } from '../../../../src/services/transport';

// ── McpServerId in tests ──────────────────────────────────────────────────────
// McpServerId is `number` in production (src/services/transport/types/ids.ts),
// but this suite registers servers under string ids on purpose: the ids embed
// straight into the namespaced tool name (`mcp_${serverId}_${tool}`), and ids
// like 'my.server' are what exercise the sanitizer's normalisation of dots and
// spaces. Vitest erased the mismatch — these files were never type-checked —
// so the cast is made explicit here rather than silently spread across ~30
// call sites. If the defensive server-id sanitization is genuinely unreachable
// now that ids are numeric, this helper is where that conversation starts.
const srv = (id: string) => id as unknown as McpServerId;


// =============================================================================
// Module mocks (same pattern as mcpIntegration.test.ts)
// =============================================================================

const transport = vi.hoisted(() => ({
  listMcpServers: vi.fn(),
}));

vi.mock('../../../../src/services/transport', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../../../src/services/transport')>()),
  getTransport: () => transport,
}));

vi.mock('../../../../src/utils/mcp', () => ({
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
 * The registered tools by the names the daemon knows them by
 * (`serverId:name`), or `null` when none is registered: the names the
 * composer's Tools ▸ saves with a chat.
 */
function toolFilter(): string[] | null {
  const registry = getToolRegistry();
  const listed = registry.getDefinitions();
  if (listed.length === 0) return null;
  return listed.map((def) => registry.getBackendName(def.function.name));
}

// =============================================================================
// Tests
// =============================================================================

describe('MCP tool name sanitization round-trip', () => {
  beforeEach(() => {
    resetToolRegistry();
    mockWarn.mockClear();
  });

  // ── Core proof ─────────────────────────────────────────────────────────────

  it('names the tool to the daemon by its exact original name, not the sanitized key', () => {
    // Tool 'get-weather.v2' from server 'test' sanitizes to 'mcp_test_get-weather_v2'
    registerMcpTools(srv('test'), [makeTool('get-weather.v2')]);
    const registry = getToolRegistry();

    // The LLM is shown the sanitized key…
    expect(registry.getDefinitions().map((d) => d.function.name)).toEqual(['mcp_test_get-weather_v2']);
    // …and the daemon is told the server and the raw original name.
    expect(registry.getBackendName('mcp_test_get-weather_v2')).toBe('test:get-weather.v2');

    expect(toolFilter()).toEqual(['test:get-weather.v2']);
  });

  // ── Unknown / unregistered tool name ──────────────────────────────────────

  it('has no original name for a sanitized name that was never registered', () => {
    const registry = getToolRegistry();

    expect(registry.has('mcp_ghost_server_nonexistent')).toBe(false);
    expect(registry.getOriginalName('mcp_ghost_server_nonexistent')).toBeUndefined();
    // With nothing to map, the name is passed through as it is.
    expect(registry.getBackendName('mcp_ghost_server_nonexistent')).toBe('mcp_ghost_server_nonexistent');
  });

  // ── Multiple tools from the same server ───────────────────────────────────

  it('names multiple tools from the same server by their respective original names', () => {
    registerMcpTools(srv('multi'), [
      makeTool('get-weather.v2'),
      makeTool('list files'),
    ]);

    expect(toolFilter()).toEqual(['multi:get-weather.v2', 'multi:list files']);
  });

  // ── Two servers, same tool name (namespace isolation) ─────────────────────

  it('names tools with the same name from different servers by the correct server', () => {
    registerMcpTools(srv('alpha'), [makeTool('ping')]);
    registerMcpTools(srv('beta'),  [makeTool('ping')]);

    expect(toolFilter()).toEqual(['alpha:ping', 'beta:ping']);
  });

  // ── Collision: skipped tools must not be offered ──────────────────────────

  it('does not register either tool when two names collide, so neither can be ticked or named', () => {
    // 'get!data' and 'get?data' both sanitize to 'mcp_s_get_data' — both are skipped
    registerMcpTools(srv('s'), [makeTool('get!data'), makeTool('get?data')]);
    const registry = getToolRegistry();

    expect(registry.has('mcp_s_get_data')).toBe(false);
    expect(registry.getOriginalName('mcp_s_get_data')).toBeUndefined();
    // No tool registered means nothing to name, not a name for the collision.
    expect(toolFilter()).toBeNull();
  });

  // ── Clean name requires no sanitization ───────────────────────────────────

  it('works correctly when the tool name needs no sanitization at all', () => {
    registerMcpTools(srv('srv'), [makeTool('get_current_time')]);

    expect(toolFilter()).toEqual(['srv:get_current_time']);
  });
});
