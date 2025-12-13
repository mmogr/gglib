/**
 * MCP (Model Context Protocol) transport sub-interface.
 * Handles MCP server lifecycle and tool invocation.
 */

import type { McpServerId } from './ids';

/**
 * MCP server type.
 */
export type McpServerType = 'stdio' | 'sse';

/**
 * MCP server status.
 */
export type McpServerStatus = 'stopped' | 'starting' | 'running' | { error: string };

/**
 * Environment variable entry for MCP server.
 */
export interface McpEnvEntry {
  key: string;
  value: string;
}

/**
 * MCP server configuration.
 */
export interface McpServerConfig {
  command?: string;
  args?: string[];
  working_dir?: string;
  url?: string;
}

/**
 * MCP server entity.
 */
export interface McpServer {
  id: McpServerId;
  name: string;
  server_type: McpServerType;
  config: McpServerConfig;
  enabled: boolean;
  auto_start: boolean;
  env: McpEnvEntry[];
  created_at: string;
  last_connected_at?: string;
}

/**
 * Parameters for creating a new MCP server.
 */
export interface NewMcpServer {
  name: string;
  server_type: McpServerType;
  config: McpServerConfig;
  enabled: boolean;
  auto_start: boolean;
  env: McpEnvEntry[];
}

/**
 * Partial update for an existing MCP server.
 * All fields are optional - only provided fields are updated.
 */
export interface UpdateMcpServer {
  name?: string;
  server_type?: McpServerType;
  config?: McpServerConfig;
  enabled?: boolean;
  auto_start?: boolean;
  env?: McpEnvEntry[];
}

/**
 * MCP tool definition.
 */
export interface McpTool {
  name: string;
  description?: string;
  input_schema?: Record<string, unknown>;
}

/**
 * MCP server with runtime info.
 */
export interface McpServerInfo {
  server: McpServer;
  status: McpServerStatus;
  tools: McpTool[];
}

/**
 * Result of calling an MCP tool.
 */
export interface McpToolResult {
  success: boolean;
  data?: unknown;
  error?: string;
}

/**
 * MCP transport operations.
 */
export interface McpTransport {
  /** List all configured MCP servers with their status. */
  listMcpServers(): Promise<McpServerInfo[]>;

  /** Add a new MCP server configuration. */
  addMcpServer(server: NewMcpServer): Promise<McpServer>;

  /** Update an existing MCP server configuration. */
  updateMcpServer(id: McpServerId, updates: UpdateMcpServer): Promise<McpServer>;

  /** Remove an MCP server configuration. */
  removeMcpServer(id: McpServerId): Promise<void>;

  /** Start an MCP server and return its available tools. */
  startMcpServer(id: McpServerId): Promise<McpTool[]>;

  /** Stop an MCP server. */
  stopMcpServer(id: McpServerId): Promise<void>;

  /** List all available tools across running MCP servers. */
  listMcpTools(): Promise<McpTool[]>;

  /** Call an MCP tool on a specific server. */
  callMcpTool(serverId: McpServerId, toolName: string, args: Record<string, unknown>): Promise<McpToolResult>;
}
