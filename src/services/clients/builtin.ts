/**
 * Built-in Tool Client
 *
 * Thin wrapper for built-in tool transport operations.
 * Delegates to getTransport() for platform-agnostic discovery.
 */

import { getTransport } from '../transport';
import type { McpTool } from '../transport/types/mcp';

// Re-export type for consumer convenience
export type { McpTool };

/**
 * Return the definitions for all backend built-in tools.
 */
export async function listBuiltinTools(): Promise<McpTool[]> {
  return getTransport().listBuiltinTools();
}
