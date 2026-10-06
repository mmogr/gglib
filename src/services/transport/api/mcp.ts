/**
 * MCP API module.
 * Handles Model Context Protocol server configuration and lifecycle.
 */

import { get, post, put, del } from './client';
import type { McpServerId } from '../types/ids';
import type {
  NewMcpServer,
  UpdateMcpServer,
  McpServerInfo,
  ResolutionStatus,
  McpTestResult,
} from '../types/mcp';
import type { CreateMcpServerRequest } from '../../../types/generated/CreateMcpServerRequest';
import type { UpdateMcpServerRequest } from '../../../types/generated/UpdateMcpServerRequest';

/**
 * List all configured MCP servers with their status.
 */
export async function listMcpServers(): Promise<McpServerInfo[]> {
  return get<McpServerInfo[]>('/api/mcp/servers');
}

/**
 * Add a new MCP server configuration.
 */
export async function addMcpServer(server: NewMcpServer): Promise<McpServerInfo> {
  // The nested config flattened into the request. The daemon requires `name`
  // and `server_type` alone; the four optional strings are left out when empty.
  const request: Pick<CreateMcpServerRequest, 'name' | 'server_type'> &
    Partial<CreateMcpServerRequest> = {
    name: server.name,
    server_type: server.server_type,
    command: server.config.command || undefined,
    args: server.config.args || [],
    working_dir: server.config.working_dir || undefined,
    path_extra: server.config.path_extra || undefined,
    url: server.config.url || undefined,
    env: server.env.map(({ key, value }) => ({ key, value })),
    lifecycle: server.lifecycle,
  };
  return post<McpServerInfo>('/api/mcp/servers', request);
}

/**
 * Update an existing MCP server configuration.
 */
export async function updateMcpServer(
  id: McpServerId,
  updates: UpdateMcpServer
): Promise<McpServerInfo> {
  // The nested config flattened into the request; a key left out is a field left alone.
  const request: Partial<UpdateMcpServerRequest> = {};
  if (updates.name !== undefined) request.name = updates.name;
  if (updates.config?.command !== undefined) request.command = updates.config.command;
  if (updates.config?.args !== undefined) request.args = updates.config.args;
  if (updates.config?.working_dir !== undefined) request.working_dir = updates.config.working_dir;
  if (updates.config?.path_extra !== undefined) request.path_extra = updates.config.path_extra;
  if (updates.config?.url !== undefined) request.url = updates.config.url;
  if (updates.env !== undefined) {
    request.env = updates.env.map(({ key, value }) => ({ key, value }));
  }
  if (updates.enabled !== undefined) request.enabled = updates.enabled;
  if (updates.lifecycle !== undefined) request.lifecycle = updates.lifecycle;
  
  return put<McpServerInfo>(`/api/mcp/servers/${id}`, request);
}

/**
 * Remove an MCP server configuration.
 */
export async function removeMcpServer(id: McpServerId): Promise<void> {
  await del<void>(`/api/mcp/servers/${id}`);
}

/**
 * Start an MCP server, answering with its server info — including the tools
 * the started instance advertises.
 */
export async function startMcpServer(id: McpServerId): Promise<McpServerInfo> {
  return post<McpServerInfo>(`/api/mcp/servers/${id}/start`);
}

/**
 * Stop an MCP server, answering with its server info in the stopped state.
 */
export async function stopMcpServer(id: McpServerId): Promise<McpServerInfo> {
  return post<McpServerInfo>(`/api/mcp/servers/${id}/stop`);
}

/**
 * Resolve MCP server executable path (for diagnostics/auto-fix).
 * Returns resolution status with success flag and detailed attempts.
 */
export async function resolveMcpServerPath(id: McpServerId): Promise<ResolutionStatus> {
  return post<ResolutionStatus>(`/api/mcp/servers/${id}/resolve`, {});
}

/**
 * Test a server's stored configuration — `gglib mcp test`.
 *
 * Starts a throwaway instance, lists its tools, stops it. Unlike starting the
 * server for real, this answers "is this config right?" without leaving a
 * process running, and it is the only way to find out short of a chat that
 * silently has no tools.
 */
export async function testMcpServer(id: McpServerId): Promise<McpTestResult> {
  return post<McpTestResult>(`/api/mcp/servers/${id}/test`, {});
}
