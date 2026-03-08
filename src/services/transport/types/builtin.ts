/**
 * Built-in tool transport sub-interface.
 * Handles discovery of statically-defined backend built-in tools.
 */

import type { McpTool } from './mcp';

/**
 * Built-in tool transport operations.
 */
export interface BuiltinTransport {
  /** Return the definitions for all backend built-in tools. */
  listBuiltinTools(): Promise<McpTool[]>;
}
