/**
 * MCP API module.
 * Handles Model Context Protocol server lifecycle and tool invocation.
 */

import { get, post, put, del } from './client';
import type { McpServerId } from '../types/ids';
import type {
  NewMcpServer,
  UpdateMcpServer,
  McpServerInfo,
  McpToolResult,
  ResolutionStatus,
  McpTestResult,
} from '../types/mcp';

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
  // Convert NewMcpServer to CreateMcpServerRequest format expected by backend
  const request = {
    name: server.name,
    server_type: server.server_type,
    command: server.config.command || undefined,
    args: server.config.args || [],
    working_dir: server.config.working_dir || undefined,
    path_extra: server.config.path_extra || undefined,
    url: server.config.url || undefined,
    env: server.env.map(e => [e.key, e.value] as [string, string]),
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
  // Convert UpdateMcpServer to UpdateMcpServerRequest format expected by backend
  const request: Record<string, unknown> = {};
  if (updates.name !== undefined) request.name = updates.name;
  if (updates.config?.command !== undefined) request.command = updates.config.command;
  if (updates.config?.args !== undefined) request.args = updates.config.args;
  if (updates.config?.working_dir !== undefined) request.working_dir = updates.config.working_dir;
  if (updates.config?.path_extra !== undefined) request.path_extra = updates.config.path_extra;
  if (updates.config?.url !== undefined) request.url = updates.config.url;
  if (updates.env !== undefined) {
    request.env = updates.env.map(e => [e.key, e.value] as [string, string]);
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
 * Call an MCP tool on a specific server.
 * 
 * Note: The backend returns {success, data, error} but the HTTP client's readData()
 * function unwraps the `data` field. We handle both wrapped and unwrapped responses.
 */
export async function callMcpTool(
  serverId: McpServerId,
  toolName: string,
  args: Record<string, unknown>
): Promise<McpToolResult> {
  try {
    const result = await post<unknown>('/api/mcp/tools/call', {
      server_id: serverId,
      tool_name: toolName,
      arguments: args,
    });
    
    // Check if result is already the full McpToolResult structure
    if (typeof result === 'object' && result !== null && 'success' in result) {
      return result as McpToolResult;
    }
    
    // Result was unwrapped by readData() - it's just the data field
    // This means the call succeeded (otherwise readData would have thrown)
    return {
      success: true,
      data: result,
      // `null`, not `undefined`: the handler builds this field on both paths,
      // so a successful call sends `"error": null` and the client's own
      // success value has to look like the one the wire produces.
      error: null,
    };
  } catch (error) {
    // Network or HTTP error - convert to McpToolResult format
    const message = error instanceof Error ? error.message : String(error);
    return {
      success: false,
      // `null` for the same reason `error` is on the success path: the handler
      // builds both fields on both paths, so a real failure carries
      // `"data": null`. `undefined` serialises to an absent key — a third
      // shape neither side describes.
      data: null,
      error: message,
    };
  }
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
