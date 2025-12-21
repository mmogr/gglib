/**
 * MCP Tool Integration for the Tool Registry.
 *
 * Bridges MCP servers to the tool registry, handling:
 * - Registering/unregistering tools when servers start/stop
 * - Converting MCP tool definitions to registry format
 * - Creating executors that call MCP servers
 */

import {
  listMcpServers,
  callMcpTool,
  isServerRunning,
} from '../clients/mcp';
import type { McpTool, McpServerId } from '../clients/mcp';
import { getToolRegistry, ToolSource } from './registry';
import type { ToolDefinition, ToolExecutor, ToolResult } from './types';

/**
 * Convert an MCP tool to a ToolDefinition.
 */
function mcpToolToDefinition(tool: McpTool): ToolDefinition {
  return {
    type: 'function',
    function: {
      name: tool.name,
      description: tool.description || `MCP tool: ${tool.name}`,
      parameters: tool.input_schema as ToolDefinition['function']['parameters'],
    },
  };
}

/**
 * Create an executor that calls an MCP tool.
 */
function createMcpExecutor(serverId: McpServerId, toolName: string): ToolExecutor {
  return async (args: Record<string, unknown>): Promise<ToolResult> => {
    try {
      const result = await callMcpTool(serverId, toolName, args);
      
      if (result.success) {
        return {
          success: true,
          data: result.data,
        };
      } else {
        return {
          success: false,
          error: result.error || 'Unknown error from MCP tool',
        };
      }
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      return {
        success: false,
        error: `MCP call failed: ${message}`,
      };
    }
  };
}

/**
 * Get the tool source ID for an MCP server.
 */
export function getMcpSource(serverId: McpServerId): ToolSource {
  return `mcp:${serverId}`;
}

/**
 * Register tools from an MCP server into the tool registry.
 *
 * @param serverId - The MCP server ID
 * @param tools - Tools discovered from the server
 * @returns Number of tools registered
 */
export function registerMcpTools(serverId: McpServerId, tools: McpTool[]): number {
  const registry = getToolRegistry();
  const source = getMcpSource(serverId);
  let count = 0;

  for (const tool of tools) {
    const definition = mcpToolToDefinition(tool);
    const executor = createMcpExecutor(serverId, tool.name);

    try {
      // Use a namespaced name to avoid collisions: mcp_serverId_toolName
      const namespacedName = `mcp_${serverId}_${tool.name}`;
      const namespacedDef: ToolDefinition = {
        ...definition,
        function: {
          ...definition.function,
          name: namespacedName,
          // Keep original description but add server context
          description: definition.function.description 
            ? `[MCP:${serverId}] ${definition.function.description}`
            : `MCP tool from server ${serverId}`,
        },
      };

      registry.register(namespacedDef, executor, source);
      count++;
    } catch (err) {
      // Tool might already exist from another source - log and continue
      console.warn(`Failed to register MCP tool ${tool.name}:`, err);
    }
  }

  return count;
}

/**
 * Unregister all tools from an MCP server.
 *
 * @param serverId - The MCP server ID
 * @returns Number of tools unregistered
 */
export function unregisterMcpTools(serverId: McpServerId): number {
  const registry = getToolRegistry();
  const source = getMcpSource(serverId);
  return registry.unregisterBySource(source);
}

/**
 * Sync tools from all running MCP servers to the registry.
 *
 * This is useful on startup or when the registry needs to be refreshed.
 * It will unregister all existing MCP tools and re-register from current state.
 */
export async function syncAllMcpTools(): Promise<{ added: number; removed: number }> {
  const registry = getToolRegistry();
  
  // First, remove all existing MCP tools
  let removed = 0;
  const sources = registry.getSourceStats();
  for (const source of sources.keys()) {
    if (source.startsWith('mcp:')) {
      removed += registry.unregisterBySource(source);
    }
  }

  // Then, get all running servers and register their tools
  let added = 0;
  try {
    const servers = await listMcpServers();
    for (const info of servers) {
      if (isServerRunning(info)) {
        added += registerMcpTools(info.server.id, info.tools);
      }
    }
  } catch (err) {
    console.error('Failed to sync MCP tools:', err);
  }

  return { added, removed };
}

/**
 * Get MCP tool info grouped by server.
 *
 * Returns a structure suitable for displaying in the ToolsPopover.
 */
export function getMcpToolsByServer(): Map<string, { serverName: string; tools: ToolDefinition[] }> {
  const registry = getToolRegistry();
  const result = new Map<string, { serverName: string; tools: ToolDefinition[] }>();

  // Get all sources and filter to MCP ones
  const stats = registry.getSourceStats();
  for (const source of stats.keys()) {
    if (source.startsWith('mcp:')) {
      const serverId = source.replace('mcp:', '');
      const tools = registry.getBySource(source);
      result.set(serverId, {
        serverName: serverId, // Could be enhanced to look up actual server name
        tools,
      });
    }
  }

  return result;
}

/**
 * Hook-friendly function to observe when MCP tools change.
 * Call this after starting/stopping MCP servers.
 */
export type McpToolsChangeCallback = (stats: { total: number; byServer: Map<string, number> }) => void;

const changeCallbacks: Set<McpToolsChangeCallback> = new Set();

/**
 * Subscribe to MCP tool changes.
 * @returns Unsubscribe function
 */
export function onMcpToolsChange(callback: McpToolsChangeCallback): () => void {
  changeCallbacks.add(callback);
  return () => changeCallbacks.delete(callback);
}

/**
 * Notify all subscribers of tool changes.
 * Call this after registering/unregistering tools.
 */
export function notifyMcpToolsChanged(): void {
  const registry = getToolRegistry();
  const stats = registry.getSourceStats();
  
  let total = 0;
  const byServer = new Map<string, number>();
  
  for (const [source, count] of stats) {
    if (source.startsWith('mcp:')) {
      const serverId = source.replace('mcp:', '');
      byServer.set(serverId, count);
      total += count;
    }
  }

  for (const callback of changeCallbacks) {
    try {
      callback({ total, byServer });
    } catch (err) {
      console.error('MCP tools change callback error:', err);
    }
  }
}
