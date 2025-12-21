/**
 * React hook for managing MCP servers.
 *
 * Provides state management and operations for MCP server configurations.
 */

import { useState, useEffect, useCallback } from "react";
import {
  listMcpServers,
  addMcpServer,
  updateMcpServer,
  removeMcpServer,
  startMcpServer,
  stopMcpServer,
  callMcpTool,
} from "../services/clients/mcp";
import type {
  McpServer,
  NewMcpServer,
  McpServerInfo,
  McpTool,
  UpdateMcpServer,
  McpServerId,
} from "../services/clients/mcp";
import { syncAllMcpTools } from "../services/tools";

interface UseMcpServersResult {
  /** List of all MCP servers with their status */
  servers: McpServerInfo[];
  /** Whether the initial load is in progress */
  loading: boolean;
  /** Error message if any operation failed */
  error: string | null;
  /** Refresh the server list */
  refresh: () => Promise<void>;
  /** Add a new server */
  addServer: (server: NewMcpServer) => Promise<McpServer>;
  /** Update an existing server */
  updateServer: (id: McpServerId, updates: UpdateMcpServer) => Promise<void>;
  /** Remove a server */
  removeServer: (id: McpServerId) => Promise<void>;
  /** Start a server */
  startServer: (id: McpServerId) => Promise<McpTool[]>;
  /** Stop a server */
  stopServer: (id: McpServerId) => Promise<void>;
}

/**
 * Hook for managing MCP servers.
 *
 * @example
 * ```tsx
 * function McpSettings() {
 *   const { servers, loading, error, addServer, startServer, stopServer } = useMcpServers();
 *
 *   if (loading) return <div>Loading...</div>;
 *   if (error) return <div>Error: {error}</div>;
 *
 *   return (
 *     <div>
 *       {servers.map(info => (
 *         <div key={info.server.id}>
 *           {info.server.name} - {info.status}
 *           <button onClick={() => startServer(info.server.id)}>
 *             Start
 *           </button>
 *         </div>
 *       ))}
 *     </div>
 *   );
 * }
 * ```
 */
export function useMcpServers(): UseMcpServersResult {
  const [servers, setServers] = useState<McpServerInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      setError(null);
      const result = await listMcpServers();
      setServers(result);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to load MCP servers");
    } finally {
      setLoading(false);
    }
  }, []);

  // Initial load
  useEffect(() => {
    refresh();
  }, [refresh]);

  const addServer = useCallback(
    async (server: NewMcpServer) => {
      setError(null);
      try {
        const result = await addMcpServer(server);
        await refresh();
        return result;
      } catch (e) {
        const msg = e instanceof Error ? e.message : "Failed to add server";
        setError(msg);
        throw e;
      }
    },
    [refresh]
  );

  const updateServerFn = useCallback(
    async (id: McpServerId, updates: UpdateMcpServer) => {
      setError(null);
      try {
        await updateMcpServer(id, updates);
        await refresh();
      } catch (e) {
        const msg = e instanceof Error ? e.message : "Failed to update server";
        setError(msg);
        throw e;
      }
    },
    [refresh]
  );

  const removeServerFn = useCallback(
    async (id: McpServerId) => {
      setError(null);
      try {
        await removeMcpServer(id);
        await refresh();
      } catch (e) {
        const msg = e instanceof Error ? e.message : "Failed to remove server";
        setError(msg);
        throw e;
      }
    },
    [refresh]
  );

  const startServerFn = useCallback(
    async (id: McpServerId) => {
      setError(null);
      try {
        const tools = await startMcpServer(id);
        await refresh();
        // Sync MCP tools to the tool registry so they're available for chat
        await syncAllMcpTools();
        return tools;
      } catch (e) {
        const msg = e instanceof Error ? e.message : "Failed to start server";
        setError(msg);
        throw e;
      }
    },
    [refresh]
  );

  const stopServerFn = useCallback(
    async (id: McpServerId) => {
      setError(null);
      try {
        await stopMcpServer(id);
        await refresh();
        // Sync MCP tools to remove stopped server's tools from registry
        await syncAllMcpTools();
      } catch (e) {
        const msg = e instanceof Error ? e.message : "Failed to stop server";
        setError(msg);
        throw e;
      }
    },
    [refresh]
  );

  return {
    servers,
    loading,
    error,
    refresh,
    addServer,
    updateServer: updateServerFn,
    removeServer: removeServerFn,
    startServer: startServerFn,
    stopServer: stopServerFn,
  };
}

/**
 * Hook for getting all available MCP tools.
 *
 * Aggregates tools from all running MCP servers.
 */
export function useMcpTools() {
  const [tools, setTools] = useState<(McpTool & { server_id: McpServerId })[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      setError(null);
      // Get all servers and aggregate their tools
      const servers = await listMcpServers();
      const flat: (McpTool & { server_id: McpServerId })[] = [];
      for (const info of servers) {
        for (const tool of info.tools) {
          flat.push({ ...tool, server_id: info.server.id });
        }
      }
      setTools(flat);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to load MCP tools");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const callToolFn = useCallback(
    async (serverId: McpServerId, toolName: string, args: Record<string, unknown>) => {
      return callMcpTool(serverId, toolName, args);
    },
    []
  );

  return {
    tools,
    loading,
    error,
    refresh,
    callTool: callToolFn,
  };
}

export default useMcpServers;
