/**
 * An MCP server as every server route reports it.
 *
 * All five — list, add, update, start, stop — answer with the nested
 * `McpServerInfo`, so a fixture standing in for any of them is `{server,
 * status, state, tools}` and not a bare row. Mocks that returned the row were
 * describing a response no handler produces; two of them also omitted
 * `config`, `created_at` and `is_valid`, which only typechecked because the
 * mirror had those fields wrong too.
 */
import type { McpServer, McpServerInfo } from '../../../src/services/transport/types/mcp';

const BASE: McpServer = {
  id: 1,
  name: 'Test Server',
  config: { command: 'echo', args: [] },
  enabled: true,
  allow_remote: false,
  lifecycle: 'lazy',
  env_keys: [],
  created_at: '2024-01-01T00:00:00Z',
  is_valid: true,
};

/** A stored server row with `overrides` applied. */
export function mcpServer(overrides: Partial<McpServer> = {}): McpServer {
  return { ...BASE, ...overrides };
}

/** The state the daemon serves beside `status` (`McpState::of`): an error is Failed. */
export function stateOf(status: McpServerInfo['status']): McpServerInfo['state'] {
  return typeof status === 'object' ? 'failed' : status;
}

/**
 * The nested response, defaulting to a stopped server offering no tools,
 * with the state the daemon would serve for its status unless one is given.
 */
export function mcpServerInfo(overrides: Partial<McpServerInfo> = {}): McpServerInfo {
  const status = overrides.status ?? 'stopped';
  return { server: BASE, status, state: stateOf(status), tools: [], ...overrides };
}
