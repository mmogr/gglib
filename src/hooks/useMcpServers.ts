/**
 * React hook for managing MCP servers.
 *
 * Provides state management and operations for MCP server configurations.
 */

import { useState, useEffect, useCallback } from "react";
import {
  McpService,
  McpServerConfig,
  McpServerInfo,
  McpTool,
} from "../services/mcp";
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
  addServer: (config: Omit<McpServerConfig, "id">) => Promise<McpServerConfig>;
  /** Update an existing server */
  updateServer: (id: string, config: McpServerConfig) => Promise<void>;
  /** Remove a server */
  removeServer: (id: string) => Promise<void>;
  /** Start a server */
  startServer: (id: string) => Promise<McpTool[]>;
  /** Stop a server */
  stopServer: (id: string) => Promise<void>;
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
 *       {servers.map(server => (
 *         <div key={server.config.id}>
 *           {server.config.name} - {server.status}
 *           <button onClick={() => startServer(server.config.id!.toString())}>
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
      const result = await McpService.listServers();
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
    async (config: Omit<McpServerConfig, "id">) => {
      setError(null);
      try {
        const result = await McpService.addServer(config);
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

  const updateServer = useCallback(
    async (id: string, config: McpServerConfig) => {
      setError(null);
      try {
        await McpService.updateServer(id, config);
        await refresh();
      } catch (e) {
        const msg = e instanceof Error ? e.message : "Failed to update server";
        setError(msg);
        throw e;
      }
    },
    [refresh]
  );

  const removeServer = useCallback(
    async (id: string) => {
      setError(null);
      try {
        await McpService.removeServer(id);
        await refresh();
      } catch (e) {
        const msg = e instanceof Error ? e.message : "Failed to remove server";
        setError(msg);
        throw e;
      }
    },
    [refresh]
  );

  const startServer = useCallback(
    async (id: string) => {
      setError(null);
      try {
        const tools = await McpService.startServer(id);
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

  const stopServer = useCallback(
    async (id: string) => {
      setError(null);
      try {
        await McpService.stopServer(id);
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
    updateServer,
    removeServer,
    startServer,
    stopServer,
  };
}

/**
 * Hook for getting all available MCP tools.
 *
 * Aggregates tools from all running MCP servers.
 */
export function useMcpTools() {
  const [tools, setTools] = useState<(McpTool & { server_id: string })[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      setError(null);
      const result = await McpService.getAllToolsFlat();
      setTools(result);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to load MCP tools");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const callTool = useCallback(
    async (serverId: string, toolName: string, args: Record<string, unknown>) => {
      return McpService.callTool(serverId, toolName, args);
    },
    []
  );

  return {
    tools,
    loading,
    error,
    refresh,
    callTool,
  };
}

export default useMcpServers;
