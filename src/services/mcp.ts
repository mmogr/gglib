/**
 * MCP (Model Context Protocol) Server Management Service
 *
 * Provides TypeScript bindings for MCP server management.
 * Auto-detects platform: Tauri desktop app uses invoke(), Web UI uses REST API.
 */

import { invoke } from "@tauri-apps/api/core";
import { getApiBase } from "../utils/apiBase";
import { isTauriApp } from "../utils/platform";

// ============================================================================
// Types
// ============================================================================

/**
 * Type of MCP server connection.
 */
export type McpServerType = "stdio" | "sse";

/**
 * Runtime status of an MCP server.
 */
export type McpServerStatus = "stopped" | "starting" | "running" | { error: string };

/**
 * Configuration for an MCP server.
 */
export interface McpServerConfig {
  /** Unique identifier (set by database on insert) */
  id?: number;
  /** User-friendly name for the server */
  name: string;
  /** Connection type (stdio or sse) */
  type: McpServerType;
  /** Whether tools from this server are included in chat */
  enabled: boolean;
  /** Whether to start this server when gglib launches */
  auto_start: boolean;
  /** Command to run (for stdio servers) */
  command?: string;
  /** Arguments to pass to the command */
  args?: string[];
  /** Working directory for the process */
  cwd?: string;
  /** URL for SSE connection */
  url?: string;
  /** Environment variables as key-value pairs */
  env: [string, string][];
  /** When the server was added */
  created_at?: string;
  /** Last successful connection time */
  last_connected_at?: string;
}

/**
 * Server information including runtime status.
 */
export interface McpServerInfo {
  /** Server configuration */
  config: McpServerConfig;
  /** Current runtime status */
  status: McpServerStatus;
  /** List of tools (if running) */
  tools: McpTool[];
}

/**
 * Tool definition from an MCP server.
 */
export interface McpTool {
  /** Tool name (function name) */
  name: string;
  /** Human-readable description */
  description?: string;
  /** JSON Schema for input parameters */
  input_schema?: Record<string, unknown>;
}

/**
 * Result of a tool call.
 */
export interface McpToolResult {
  /** Whether the call succeeded */
  success: boolean;
  /** Result data (if success) */
  data?: unknown;
  /** Error message (if failed) */
  error?: string;
}

// ============================================================================
// Helper Functions
// ============================================================================

async function apiFetch(path: string, init?: RequestInit): Promise<Response> {
  const apiBase = await getApiBase();
  return fetch(`${apiBase}${path}`, init);
}

interface ApiResponse<T> {
  success: boolean;
  data?: T;
  error?: string;
}

// ============================================================================
// MCP Service
// ============================================================================

export class McpService {
  // =========================================================================
  // Configuration CRUD
  // =========================================================================

  /**
   * Add a new MCP server configuration.
   */
  static async addServer(config: Omit<McpServerConfig, "id">): Promise<McpServerConfig> {
    if (isTauriApp) {
      return await invoke<McpServerConfig>("add_mcp_server", { config });
    } else {
      const response = await apiFetch("/mcp/servers", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(config),
      });

      if (!response.ok) {
        const error: ApiResponse<unknown> = await response.json();
        throw new Error(error.error || "Failed to add MCP server");
      }

      const data: ApiResponse<McpServerConfig> = await response.json();
      if (!data.data) {
        throw new Error("No data returned from server");
      }
      return data.data;
    }
  }

  /**
   * List all MCP server configurations with their current status.
   */
  static async listServers(): Promise<McpServerInfo[]> {
    if (isTauriApp) {
      return await invoke<McpServerInfo[]>("list_mcp_servers");
    } else {
      const response = await apiFetch("/mcp/servers");

      if (!response.ok) {
        throw new Error(`Failed to list MCP servers: ${response.statusText}`);
      }

      const data: ApiResponse<McpServerInfo[]> = await response.json();
      return data.data || [];
    }
  }

  /**
   * Update an existing MCP server configuration.
   */
  static async updateServer(id: string, config: McpServerConfig): Promise<McpServerConfig> {
    if (isTauriApp) {
      return await invoke<McpServerConfig>("update_mcp_server", { id, config });
    } else {
      const response = await apiFetch(`/mcp/servers/${id}`, {
        method: "PUT",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(config),
      });

      if (!response.ok) {
        const error: ApiResponse<unknown> = await response.json();
        throw new Error(error.error || "Failed to update MCP server");
      }

      const data: ApiResponse<McpServerConfig> = await response.json();
      if (!data.data) {
        throw new Error("No data returned from server");
      }
      return data.data;
    }
  }

  /**
   * Remove an MCP server configuration.
   */
  static async removeServer(id: string): Promise<void> {
    if (isTauriApp) {
      await invoke("remove_mcp_server", { id });
    } else {
      const response = await apiFetch(`/mcp/servers/${id}`, {
        method: "DELETE",
      });

      if (!response.ok) {
        const error: ApiResponse<unknown> = await response.json();
        throw new Error(error.error || "Failed to remove MCP server");
      }
    }
  }

  // =========================================================================
  // Server Lifecycle
  // =========================================================================

  /**
   * Start an MCP server.
   * @returns List of tools discovered from the server.
   */
  static async startServer(id: string): Promise<McpTool[]> {
    if (isTauriApp) {
      return await invoke<McpTool[]>("start_mcp_server", { id });
    } else {
      const response = await apiFetch(`/mcp/servers/${id}/start`, {
        method: "POST",
      });

      if (!response.ok) {
        const error: ApiResponse<unknown> = await response.json();
        throw new Error(error.error || "Failed to start MCP server");
      }

      const data: ApiResponse<McpTool[]> = await response.json();
      return data.data || [];
    }
  }

  /**
   * Stop an MCP server.
   */
  static async stopServer(id: string): Promise<void> {
    if (isTauriApp) {
      await invoke("stop_mcp_server", { id });
    } else {
      const response = await apiFetch(`/mcp/servers/${id}/stop`, {
        method: "POST",
      });

      if (!response.ok) {
        const error: ApiResponse<unknown> = await response.json();
        throw new Error(error.error || "Failed to stop MCP server");
      }
    }
  }

  // =========================================================================
  // Tool Operations
  // =========================================================================

  /**
   * Get all tools from all running MCP servers.
   * @returns Map of server_id -> tools list
   */
  static async listAllTools(): Promise<Map<string, McpTool[]>> {
    if (isTauriApp) {
      const result = await invoke<[string, McpTool[]][]>("list_mcp_tools");
      return new Map(result);
    } else {
      const response = await apiFetch("/mcp/tools");

      if (!response.ok) {
        throw new Error(`Failed to list MCP tools: ${response.statusText}`);
      }

      const data: ApiResponse<[string, McpTool[]][]> = await response.json();
      return new Map(data.data || []);
    }
  }

  /**
   * Get a flat list of all tools from all running servers.
   * Each tool includes a server_id property for identification.
   */
  static async getAllToolsFlat(): Promise<(McpTool & { server_id: string })[]> {
    const toolsMap = await this.listAllTools();
    const flat: (McpTool & { server_id: string })[] = [];

    for (const [serverId, tools] of toolsMap) {
      for (const tool of tools) {
        flat.push({ ...tool, server_id: serverId });
      }
    }

    return flat;
  }

  /**
   * Call a tool on a specific MCP server.
   */
  static async callTool(
    serverId: string,
    toolName: string,
    args: Record<string, unknown>
  ): Promise<McpToolResult> {
    if (isTauriApp) {
      return await invoke<McpToolResult>("call_mcp_tool", {
        serverId,
        toolName,
        arguments: args,
      });
    } else {
      const response = await apiFetch(`/mcp/servers/${serverId}/tools/${toolName}`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ arguments: args }),
      });

      if (!response.ok) {
        const error: ApiResponse<unknown> = await response.json();
        throw new Error(error.error || "Failed to call MCP tool");
      }

      const data: ApiResponse<McpToolResult> = await response.json();
      if (!data.data) {
        throw new Error("No data returned from tool call");
      }
      return data.data;
    }
  }

  // =========================================================================
  // Utility Methods
  // =========================================================================

  /**
   * Create a new stdio server configuration.
   */
  static createStdioConfig(
    name: string,
    command: string,
    args: string[] = [],
    env: [string, string][] = []
  ): Omit<McpServerConfig, "id"> {
    return {
      name,
      type: "stdio",
      enabled: true,
      auto_start: false,
      command,
      args,
      env,
    };
  }

  /**
   * Create a new SSE server configuration.
   */
  static createSseConfig(
    name: string,
    url: string,
    env: [string, string][] = []
  ): Omit<McpServerConfig, "id"> {
    return {
      name,
      type: "sse",
      enabled: true,
      auto_start: false,
      url,
      env,
    };
  }

  /**
   * Check if a server is running.
   */
  static isRunning(info: McpServerInfo): boolean {
    return info.status === "running";
  }

  /**
   * Check if a server has an error.
   */
  static hasError(info: McpServerInfo): info is McpServerInfo & { status: { error: string } } {
    return typeof info.status === "object" && "error" in info.status;
  }

  /**
   * Get the error message from a server status.
   */
  static getErrorMessage(info: McpServerInfo): string | null {
    if (this.hasError(info)) {
      return info.status.error;
    }
    return null;
  }
}

export default McpService;
