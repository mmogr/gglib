/**
 * MCP Client
 *
 * Thin wrappers for MCP Transport operations.
 * Delegates to getTransport() for platform-agnostic MCP server management.
 */

import { getTransport } from '../transport';
import type { McpServerId } from '../transport/types/ids';
import type {
  McpServer,
  McpServerInfo,
  McpTool,
  McpToolResult,
  NewMcpServer,
  UpdateMcpServer,
  McpEnvEntry,
} from '../transport/types/mcp';

// Re-export types for consumer convenience
export type {
  McpServer,
  McpServerInfo,
  McpTool,
  McpToolResult,
  NewMcpServer,
  UpdateMcpServer,
  McpEnvEntry,
  McpServerId,
};

// ============================================================================
// Transport Wrappers
// ============================================================================

/**
 * List all configured MCP servers with their status.
 */
export async function listMcpServers(): Promise<McpServerInfo[]> {
  return getTransport().listMcpServers();
}

/**
 * Add a new MCP server configuration.
 */
export async function addMcpServer(server: NewMcpServer): Promise<McpServer> {
  return getTransport().addMcpServer(server);
}

/**
 * Update an existing MCP server configuration.
 */
export async function updateMcpServer(id: McpServerId, updates: UpdateMcpServer): Promise<McpServer> {
  return getTransport().updateMcpServer(id, updates);
}

/**
 * Remove an MCP server configuration.
 */
export async function removeMcpServer(id: McpServerId): Promise<void> {
  return getTransport().removeMcpServer(id);
}

/**
 * Start an MCP server and return its available tools.
 */
export async function startMcpServer(id: McpServerId): Promise<McpTool[]> {
  return getTransport().startMcpServer(id);
}

/**
 * Stop an MCP server.
 */
export async function stopMcpServer(id: McpServerId): Promise<void> {
  return getTransport().stopMcpServer(id);
}

/**
 * Call an MCP tool on a specific server.
 */
export async function callMcpTool(
  serverId: McpServerId,
  toolName: string,
  args: Record<string, unknown>
): Promise<McpToolResult> {
  return getTransport().callMcpTool(serverId, toolName, args);
}

/**
 * Resolve MCP server executable path (for diagnostics/auto-fix).
 */
export async function resolveMcpServerPath(id: McpServerId) {
  return getTransport().resolveMcpServerPath(id);
}

// ============================================================================
// Utility Functions
// ============================================================================

/**
 * Create a new stdio server configuration.
 */
export function createStdioConfig(
  name: string,
  command: string,
  args: string[] = [],
  env: McpEnvEntry[] = [],
  working_dir?: string,
  path_extra?: string
): NewMcpServer {
  return {
    name,
    server_type: 'stdio',
    config: {
      command,
      args,
      working_dir,
      path_extra,
    },
    enabled: true,
    auto_start: false,
    env,
  };
}

/**
 * Create a new SSE server configuration.
 */
export function createSseConfig(
  name: string,
  url: string,
  env: McpEnvEntry[] = []
): NewMcpServer {
  return {
    name,
    server_type: 'sse',
    config: {
      url,
    },
    enabled: true,
    auto_start: false,
    env,
  };
}

/**
 * Check if a server is running.
 */
export function isServerRunning(info: McpServerInfo): boolean {
  return info.status === 'running';
}

/**
 * Check if a server has an error.
 */
export function hasServerError(info: McpServerInfo): info is McpServerInfo & { status: { error: string } } {
  return typeof info.status === 'object' && 'error' in info.status;
}

/**
 * Get the error message from a server status.
 */
export function getServerErrorMessage(info: McpServerInfo): string | null {
  if (hasServerError(info)) {
    return info.status.error;
  }
  return null;
}
