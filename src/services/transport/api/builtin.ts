/**
 * Built-in tool API module.
 * Fetches statically-defined backend built-in tool definitions.
 */

import { get } from './client';
import type { McpTool } from '../types/mcp';

/**
 * Return the definitions for all backend built-in tools.
 */
export async function listBuiltinTools(): Promise<McpTool[]> {
  return get<McpTool[]>('/api/builtin/tools');
}
