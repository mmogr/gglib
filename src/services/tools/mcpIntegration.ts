/**
 * MCP Tool Integration for the Tool Registry.
 *
 * Bridges MCP servers to the tool registry, handling:
 * - Registering/unregistering tools when servers start/stop
 * - Converting MCP tool definitions to registry format
 * - Recording each sanitized name's server and original name, which is how
 *   a run's tool filter names the tool to the daemon
 */

import { getTransport } from '../transport';
import type { McpTool, McpServerId } from '../transport';
import { isServerRunning } from '../../utils/mcp';
import { getToolRegistry, ToolSource } from './registry';
import type { ToolDefinition } from './types';
import { sanitizeToolName, detectCollisions } from './nameUtils';
import { mcpGenericRenderer } from './renderers';
import { appLogger } from '../platform';

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

  // ── Collision detection ───────────────────────────────────────────────────
  // Build the fully-namespaced raw names for the entire batch first, so that
  // any two tools that sanitize to the same string (e.g. due to the 64-char
  // truncation or special-char normalisation) are caught before registration.
  const namespacedRawNames = tools.map((t) => `mcp_${serverId}_${t.name}`);
  const collisions = detectCollisions(namespacedRawNames);

  if (collisions.size > 0) {
    for (const [sanitized, originals] of collisions) {
      appLogger.warn('service.mcp', 'MCP tool name collision detected — skipping affected tools', {
        serverId,
        sanitizedName: sanitized,
        collidingRawNames: originals,
      });
    }
  }

  // Flatten colliding raw names into a Set for O(1) skip checks in the loop.
  const toSkip = new Set<string>([...collisions.values()].flat());

  // ── Registration loop ─────────────────────────────────────────────────────
  for (const tool of tools) {
    const namespacedRaw = `mcp_${serverId}_${tool.name}`;

    if (toSkip.has(namespacedRaw)) {
      // Already warned above; skip silently.
      continue;
    }

    const sanitizedName = sanitizeToolName(namespacedRaw);

    if (sanitizedName !== namespacedRaw) {
      appLogger.warn('service.mcp', 'MCP tool name was sanitized', {
        serverId,
        original: namespacedRaw,
        sanitized: sanitizedName,
      });
    }

    const definition = mcpToolToDefinition(tool);

    try {
      const namespacedDef: ToolDefinition = {
        ...definition,
        function: {
          ...definition.function,
          name: sanitizedName,
          description: definition.function.description
            ? `[MCP:${serverId}] ${definition.function.description}`
            : `MCP tool from server ${serverId}`,
        },
      };

      // The name map keeps tool.name (raw), never the sanitized key: it is the
      // name the daemon is given, and the MCP server only understands its own.
      registry.registerWithNameMapping(tool.name, String(serverId), sanitizedName, namespacedDef, source, mcpGenericRenderer);
      count++;
    } catch (err) {
      // Tool might already exist from another source — log and continue.
      appLogger.warn('service.mcp', 'Failed to register MCP tool', { toolName: tool.name, error: err });
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
    const servers = await getTransport().listMcpServers();
    for (const info of servers) {
      if (isServerRunning(info)) {
        added += registerMcpTools(info.server.id, info.tools);
      }
    }
  } catch (err) {
    appLogger.error('service.mcp', 'Failed to sync MCP tools', { error: err });
  }

  return { added, removed };
}
