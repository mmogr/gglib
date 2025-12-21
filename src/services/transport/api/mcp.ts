/**
 * MCP API module.
 * Handles Model Context Protocol server lifecycle and tool invocation.
 */

import { get, post, put, del } from './client';
import type { McpServerId } from '../types/ids';
import type {
  McpServer,
  NewMcpServer,
  UpdateMcpServer,
  McpServerInfo,
  McpTool,
  McpToolResult,
  ResolutionStatus,
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
export async function addMcpServer(server: NewMcpServer): Promise<McpServer> {
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
    auto_start: server.auto_start,
  };
  return post<McpServer>('/api/mcp/servers', request);
}

/**
 * Update an existing MCP server configuration.
 */
export async function updateMcpServer(
  id: McpServerId,
  updates: UpdateMcpServer
): Promise<McpServer> {
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
  if (updates.auto_start !== undefined) request.auto_start = updates.auto_start;
  
  return put<McpServer>(`/api/mcp/servers/${id}`, request);
}

/**
 * Remove an MCP server configuration.
 */
export async function removeMcpServer(id: McpServerId): Promise<void> {
  await del<void>(`/api/mcp/servers/${id}`);
}

/**
 * Start an MCP server and return its available tools.
 */
export async function startMcpServer(id: McpServerId): Promise<McpTool[]> {
  return post<McpTool[]>(`/api/mcp/servers/${id}/start`);
}

/**
 * Stop an MCP server.
 */
export async function stopMcpServer(id: McpServerId): Promise<void> {
  await post<void>(`/api/mcp/servers/${id}/stop`);
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
      error: undefined,
    };
  } catch (error) {
    // Network or HTTP error - convert to McpToolResult format
    const message = error instanceof Error ? error.message : String(error);
    return {
      success: false,
      data: undefined,
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
