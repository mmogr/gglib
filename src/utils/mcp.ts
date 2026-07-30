/**
 * MCP server status predicates.
 *
 * Pure functions over an `McpServerInfo` snapshot — no transport involvement.
 * Type-only import keeps this module free of any runtime dependency on
 * `services/transport`.
 *
 * Note: `services/serverRegistry.ts` exports an unrelated `isServerRunning`
 * that takes a model ID. These two are not interchangeable.
 *
 * @module utils/mcp
 */

import type { McpServerInfo } from '../services/transport/types/mcp';

/**
 * Check if a server is running.
 */
export function isServerRunning(info: McpServerInfo): boolean {
  return info.status === 'running';
}

/**
 * Check if a server has an error.
 */
export function hasServerError(
  info: McpServerInfo
): info is McpServerInfo & { status: { error: string } } {
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
