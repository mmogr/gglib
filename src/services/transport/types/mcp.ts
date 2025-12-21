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
  /** Command/basename to resolve (e.g., "npx" or "/usr/local/bin/python3") */
  command?: string;
  /** Cached absolute path (auto-resolved from command) */
  resolved_path_cache?: string;
  /** Command-line arguments */
  args?: string[];
  /** Working directory (must be absolute if specified) */
  working_dir?: string;
  /** Additional PATH entries for child process */
  path_extra?: string;
  /** URL for SSE connection (required for sse servers) */
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
  /** Whether server configuration is valid */
  is_valid: boolean;
  /** Last validation or runtime error */
  last_error?: string;
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
 * Resolution attempt for diagnostics.
 */
export interface ResolutionAttempt {
  /** The candidate path that was tried */
  candidate: string;
  /** The outcome of checking this candidate */
  outcome: string;
}

/**
 * Result of executable path resolution.
 */
export interface ResolutionStatus {
  /** Whether resolution succeeded */
  success: boolean;
  /** The resolved absolute path (if successful) */
  resolved_path?: string | null;
  /** All attempts made during resolution (for diagnostics) */
  attempts: ResolutionAttempt[];
  /** Non-fatal warnings */
  warnings: string[];
  /** Error message (if resolution failed) */
  error_message?: string | null;
  /** Suggested command to run to find the executable */
  suggested_fix?: string | null;
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

  /** Call an MCP tool on a specific server. */
  callMcpTool(serverId: McpServerId, toolName: string, args: Record<string, unknown>): Promise<McpToolResult>;

  /** Resolve MCP server executable path (for diagnostics/auto-fix). */
  resolveMcpServerPath(id: McpServerId): Promise<ResolutionStatus>;
}
