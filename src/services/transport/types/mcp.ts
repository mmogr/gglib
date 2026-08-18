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
import type { McpServerStatusDto as McpServerStatus } from '../../../types/generated/McpServerStatusDto';
export type { McpServerStatus };

/**
 * Environment variable entry for MCP server.
 */
import type { McpEnvEntryDto as McpEnvEntry } from '../../../types/generated/McpEnvEntryDto';
export type { McpEnvEntry };

/**
 * MCP server configuration.
 */
import type { McpServerConfigDto as McpServerConfig } from '../../../types/generated/McpServerConfigDto';
export type { McpServerConfig };

import type { McpLifecycle } from '../../../types/generated/McpLifecycle';
export type { McpLifecycle };

/**
 * MCP server entity.
 *
 * `server_type` is narrowed back by intersection: Rust stores it as a plain
 * `String`, but the two values it can hold are the two transports the config
 * union already models, and the editor switches on them. `string & Union`
 * collapses to the union.
 */
import type { McpServerDto } from '../../../types/generated/McpServerDto';
export type McpServer = McpServerDto & { server_type: McpServerType };

/**
 * Parameters for creating a new MCP server.
 */
export interface NewMcpServer {
  name: string;
  server_type: McpServerType;
  config: McpServerConfig;
  enabled: boolean;
  lifecycle: McpLifecycle;
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
  lifecycle?: McpLifecycle;
  env?: McpEnvEntry[];
}

/**
 * MCP tool definition.
 */
export interface McpTool {
  name: string;
  /**
   * Optional *and* nullable, because two Rust types feed this one shape and
   * they disagree. `McpToolInfo` (`/api/mcp/servers`, `…/test`) carries
   * `skip_serializing_if` on `title` alone, so it always emits this key —
   * `null` when there is nothing to say. `gglib_core::McpTool`
   * (`/api/builtin/tools`) carries it on all three and omits the key instead.
   */
  description?: string | null;
  /** Always present as `null` from the server routes; omitted by the builtin
   *  route — see {@link description}. */
  input_schema?: Record<string, unknown> | null;
  /** Human-readable display title from MCP annotations.title (spec 2025-03-26). */
  title?: string;
}

/**
 * MCP server with runtime info — what every server route answers with.
 */
import type { McpServerInfo as McpServerInfoDto } from '../../../types/generated/McpServerInfo';
export type McpServerInfo = Omit<McpServerInfoDto, 'server'> & { server: McpServer };

/**
 * Result of calling an MCP tool.
 *
 * `error` is a required nullable, not an optional key: the handler builds the
 * field on both paths, so a successful call sends `"error": null` rather than
 * omitting it.
 */
import type { McpToolCallResponse } from '../../../types/generated/McpToolCallResponse';
export type McpToolResult = McpToolCallResponse;

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
  addMcpServer(server: NewMcpServer): Promise<McpServerInfo>;

  /** Update an existing MCP server configuration. */
  updateMcpServer(id: McpServerId, updates: UpdateMcpServer): Promise<McpServerInfo>;

  /** Remove an MCP server configuration. */
  removeMcpServer(id: McpServerId): Promise<void>;

  /** Start an MCP server, answering with its info and advertised tools. */
  startMcpServer(id: McpServerId): Promise<McpServerInfo>;

  /** Stop an MCP server, answering with its info in the stopped state. */
  stopMcpServer(id: McpServerId): Promise<McpServerInfo>;

  /** Call an MCP tool on a specific server. */
  callMcpTool(serverId: McpServerId, toolName: string, args: Record<string, unknown>): Promise<McpToolResult>;

  /** Resolve MCP server executable path (for diagnostics/auto-fix). */
  resolveMcpServerPath(id: McpServerId): Promise<ResolutionStatus>;

  /** Start a throwaway instance to check the config, then stop it. */
  testMcpServer(id: McpServerId): Promise<McpTestResult>;
}

/**
 * The outcome of testing a server's configuration.
 *
 * A failed connection is a result, not an error: a wrong command is the
 * ordinary case this diagnoses, so `ok: false` carries the reason.
 */
import type { McpTestResult } from '../../../types/generated/McpTestResult';
export type { McpTestResult };
