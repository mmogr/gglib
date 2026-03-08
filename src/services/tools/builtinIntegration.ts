/**
 * Built-in Tool Integration for the Tool Registry.
 *
 * Fetches built-in tool definitions from the backend and registers them
 * into the ToolRegistry under the `'builtin'` source, replacing any
 * stale TypeScript-defined entries.
 *
 * Execution is handled entirely by the backend; the executor stored here
 * is a guard that should never be called in normal operation.
 */

import { listBuiltinTools } from '../clients/builtin';
import type { McpTool } from '../clients/builtin';
import { getToolRegistry } from './registry';
import type { ToolDefinition, ToolExecutor, ToolResult } from './types';
import { timeRenderer } from './renderers/TimeRenderer';
import { appLogger } from '../platform';

// ---------------------------------------------------------------------------
// Renderer map — extend this when adding new built-in tools.
// ---------------------------------------------------------------------------

const BUILTIN_RENDERERS: Record<string, import('./types').ToolResultRenderer> = {
  get_current_time: timeRenderer,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function builtinToolToDefinition(tool: McpTool): ToolDefinition {
  return {
    type: 'function',
    function: {
      name: tool.name,
      description: tool.description || `Built-in tool: ${tool.name}`,
      parameters: tool.input_schema as ToolDefinition['function']['parameters'],
    },
  };
}

/**
 * Executor placeholder for built-in tools.
 * Execution is handled by the backend agent loop — this path is never
 * reached in normal operation.
 */
function createBuiltinExecutor(toolName: string): ToolExecutor {
  return async (_args: Record<string, unknown>): Promise<ToolResult> => ({
    success: false,
    error: `Built-in tool '${toolName}' must be executed by the backend agent loop.`,
  });
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/**
 * Sync built-in tool definitions from the backend into the tool registry.
 *
 * Unregisters all existing `'builtin'` tools first, then re-registers
 * using definitions fetched from `GET /api/builtin/tools`.
 *
 * @returns Counts of added and removed tools for diagnostics.
 */
export async function syncBuiltinTools(): Promise<{ added: number; removed: number }> {
  const registry = getToolRegistry();

  // Remove stale built-in entries (including any eagerly-registered TS stubs).
  const removed = registry.unregisterBySource('builtin');

  let added = 0;
  try {
    const tools: McpTool[] = await listBuiltinTools();
    for (const tool of tools) {
      const definition = builtinToolToDefinition(tool);
      const executor = createBuiltinExecutor(tool.name);
      const renderer = BUILTIN_RENDERERS[tool.name];

      try {
        registry.registerWithNameMapping(
          tool.name,
          'builtin',
          tool.name,
          definition,
          executor,
          'builtin',
          renderer,
        );
        added++;
      } catch (err) {
        appLogger.warn('service.builtin', 'Failed to register built-in tool', {
          toolName: tool.name,
          error: err,
        });
      }
    }
  } catch (err) {
    appLogger.error('service.builtin', 'Failed to fetch built-in tools from backend', { error: err });
  }

  return { added, removed };
}
