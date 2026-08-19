/**
 * An MCP server as every server route reports it.
 *
 * All five — list, add, update, start, stop — answer with the nested
 * `McpServerInfo`, so a fixture standing in for any of them is `{server,
 * status, tools}` and not a bare row. Mocks that returned the row were
 * describing a response no handler produces; two of them also spelled
 * `server_type` as `type` and omitted `config`, `created_at` and `is_valid`,
 * which only typechecked because the mirror had those fields wrong too.
 */
import type { McpServer, McpServerInfo } from '../../../src/services/transport/types/mcp';

const BASE: McpServer = {
  id: 1,
  name: 'Test Server',
  server_type: 'stdio',
  config: { command: 'echo', args: [] },
  enabled: true,
  lifecycle: 'lazy',
  env: [],
  created_at: '2024-01-01T00:00:00Z',
  is_valid: true,
};

/** A stored server row with `overrides` applied. */
export function mcpServer(overrides: Partial<McpServer> = {}): McpServer {
  return { ...BASE, ...overrides };
}

/** The nested response, defaulting to a stopped server offering no tools. */
export function mcpServerInfo(overrides: Partial<McpServerInfo> = {}): McpServerInfo {
  return { server: BASE, status: 'stopped', tools: [], ...overrides };
}
