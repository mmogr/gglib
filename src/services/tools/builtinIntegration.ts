/**
 * Built-in Tool Integration for the Tool Registry.
 *
 * Fetches built-in tool definitions from the backend and registers them
 * into the ToolRegistry under the `'builtin'` source, replacing any
 * stale TypeScript-defined entries. Kept current (`keptCurrent`): read when
 * the window starts and again each time the event stream opens, so a first
 * read the daemon refused on a cold start is followed by one that lands.
 * The list is fetched before anything is replaced, so a read that fails
 * leaves the tools that were there, and one overtaken by a later read is
 * dropped.
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
import { keptCurrent, type Reread } from '../bridgeEvents';

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

/** Put `tools` in place of every built-in the registry holds, in one step. */
function replaceBuiltinTools(tools: McpTool[]): void {
  const registry = getToolRegistry();
  // Remove stale built-in entries (including any eagerly-registered TS stubs).
  registry.unregisterBySource('builtin');
  for (const tool of tools) {
    try {
      registry.registerWithNameMapping(
        tool.name,
        'builtin',
        tool.name,
        builtinToolToDefinition(tool),
        'builtin',
        BUILTIN_RENDERERS[tool.name],
      );
    } catch (err) {
      appLogger.warn('service.builtin', 'Failed to register built-in tool', {
        toolName: tool.name,
        error: err,
      });
    }
  }
}

const kept = keptCurrent({
  read: async () => {
    try {
      return await getTransport().listBuiltinTools();
    } catch (err) {
      appLogger.error('service.builtin', 'Failed to fetch built-in tools from backend', { error: err });
      throw err;
    }
  },
  apply: replaceBuiltinTools,
});

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/** Read the built-in tools now and each time the event stream opens. Returns the way to stop. */
export function keepBuiltinToolsCurrent(): () => void {
  return kept.keep();
}

/**
 * Read the built-in tools once, and put them in the registry in place of
 * the built-ins it held. A read that fails leaves those standing.
 */
export function syncBuiltinTools(): Promise<Reread> {
  return kept.refresh();
}
