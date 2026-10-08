/**
 * Built-in Tool Integration for the Tool Registry.
 *
 * Fetches built-in tool definitions from the backend and registers them
 * into the ToolRegistry under the `'builtin'` source, replacing any
 * stale TypeScript-defined entries.
 *
 * Execution is handled entirely by the backend: what is registered here is
 * the definition and, for the tools that have one, a result renderer.
 */

import { getTransport } from '../transport';
import type { McpTool } from '../transport';
import { getToolRegistry } from './registry';
import type { ToolDefinition } from './types';
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
    const tools: McpTool[] = await getTransport().listBuiltinTools();
    for (const tool of tools) {
      const definition = builtinToolToDefinition(tool);
      const renderer = BUILTIN_RENDERERS[tool.name];

      try {
        registry.registerWithNameMapping(
          tool.name,
          'builtin',
          tool.name,
          definition,
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
