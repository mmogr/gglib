/**
 * Tool registry: what the UI knows about the tools the daemon can run.
 *
 * It holds each tool's definition, which ones the person has enabled, the
 * daemon's name for each, and how to draw its result. It runs nothing: a run
 * names its enabled tools in `tool_filter` and the daemon executes them.
 */

import type { ToolDefinition, ToolResultRenderer, RegisteredTool } from './types';

/**
 * Source identifier for tool registration.
 * Used to track where tools came from for bulk operations.
 */
export type ToolSource = 
  | 'builtin'           // Built-in tools (datetime, etc.)
  | `mcp:${string}`;    // MCP server tools (mcp:server-id)

/**
 * Extended registered tool with source tracking.
 */
interface RegisteredToolWithSource extends RegisteredTool {
  source: ToolSource;
}

/**
 * Metadata stored in the reverse name map for each sanitized MCP tool name.
 */
export interface NameMapEntry {
  /** The original raw tool name from the MCP server (e.g. 'get data!'). */
  originalName: string;
  /** The MCP server ID that owns this tool. */
  serverId: string;
}

/**
 * Registry of the tools available to the LLM.
 * Handles tool registration, lookup and enablement.
 */
export class ToolRegistry {
  private tools = new Map<string, RegisteredToolWithSource>();
  // Secure-by-default: tools are disabled unless explicitly enabled.
  // We keep an allowlist instead of a denylist so registration cannot
  // accidentally re-enable tools (e.g., during MCP resync).
  private enabledTools = new Set<string>();

  /**
   * Reverse map: sanitized tool name → { originalName, serverId }.
   * Populated only for MCP tools registered via registerWithNameMapping().
   */
  private _nameMap = new Map<string, NameMapEntry>();

  /**
   * Register a tool with its definition.
   * @param definition - OpenAI-compatible tool definition
   * @param source - Source identifier for the tool (default: 'builtin')
   * @param renderer - Optional renderer for displaying results in the chat UI
   * @throws Error if tool with same name already exists
   */
  register(definition: ToolDefinition, source: ToolSource = 'builtin', renderer?: ToolResultRenderer): void {
    const name = definition.function.name;
    if (this.tools.has(name)) {
      // Allow silent re-registration from the same source (idempotent sync)
      const existing = this.tools.get(name)!;
      if (existing.source === source) {
        this.tools.set(name, { definition, source, renderer });
        return;
      }
      throw new Error(`Tool "${name}" is already registered`);
    }
    this.tools.set(name, { definition, source, renderer });
    // Newly registered tools are disabled by default.
    // Intentionally do not mutate enable-state here so that if a tool is
    // re-registered after being enabled (e.g., MCP resync), it stays enabled.
  }

  /**
   * Register an MCP tool and record the sanitized → original name mapping.
   *
   * Use this instead of register() for all MCP tools so that UI components
   * and routing logic can retrieve the original MCP name and owning server
   * from a sanitized name.
   *
   * @param originalName - Raw tool name from the MCP server (e.g. 'get data!')
   * @param serverId     - MCP server ID that owns this tool
   * @param sanitizedName - Sanitized name used as the registry key
   * @param definition   - ToolDefinition whose function.name must equal sanitizedName
   * @param source       - Tool source (e.g. 'mcp:server-id')
   * @param renderer     - Optional renderer for displaying results in the chat UI
   */
  registerWithNameMapping(
    originalName: string,
    serverId: string,
    sanitizedName: string,
    definition: ToolDefinition,
    source: ToolSource,
    renderer?: ToolResultRenderer,
  ): void {
    this._nameMap.set(sanitizedName, { originalName, serverId });
    this.register(definition, source, renderer);
  }

  /**
   * Look up the original MCP tool name for a sanitized registry key.
   * @returns Original raw name, or undefined if no mapping exists.
   */
  getOriginalName(sanitizedName: string): string | undefined {
    return this._nameMap.get(sanitizedName)?.originalName;
  }

  /**
   * Look up the MCP server ID that owns a sanitized tool name.
   * @returns Server ID string, or undefined if no mapping exists.
   */
  getServerId(sanitizedName: string): string | undefined {
    return this._nameMap.get(sanitizedName)?.serverId;
  }

  /**
   * Resolve the backend wire name for a sanitized registry key.
   *
   * - MCP tools registered via `registerWithNameMapping` become `serverId:originalName`
   *   (the format the backend agent handlers expect).
   * - Built-in tools (and any tool without a name mapping) keep their sanitized name.
   *
   * This is the single source of truth used by the agentic chat tool-filter.
   */
  getBackendName(sanitizedName: string): string {
    const entry = this._nameMap.get(sanitizedName);
    if (entry !== undefined) {
      return `${entry.serverId}:${entry.originalName}`;
    }
    return sanitizedName;
  }

  /**
   * Return all registered tools as `{ displayName, backendName, description }` triples.
   *
   * `displayName` is the human-readable name shown in the UI (sanitized name).
   * `backendName` is the wire name sent to the backend (see `getBackendName`).
   * `description` is the tool's LLM-facing description string.
   */
  getAllAsBackendTools(): Array<{ displayName: string; backendName: string; description: string }> {
    return Array.from(this.tools.entries()).map(([sanitized, tool]) => ({
      displayName: sanitized,
      backendName: this.getBackendName(sanitized),
      description: tool.definition.function.description ?? '',
    }));
  }

  /**
   * Register a tool using a simplified builder pattern.
   * @param name - Function name
   * @param description - Description for the LLM
   * @param parameters - JSON Schema for parameters (optional)
   * @param source - Source identifier for the tool (default: 'builtin')
   * @param renderer - Optional renderer for displaying results in the chat UI
   */
  registerFunction(
    name: string,
    description: string,
    parameters: ToolDefinition['function']['parameters'] | undefined,
    source: ToolSource = 'builtin',
    renderer?: ToolResultRenderer
  ): void {
    this.register(
      {
        type: 'function',
        function: {
          name,
          description,
          parameters,
        },
      },
      source,
      renderer
    );
  }

  /**
   * Unregister a tool by name.
   * @returns true if tool was removed, false if not found
   */
  unregister(name: string): boolean {
    return this.tools.delete(name);
  }

  /**
   * Check if a tool is registered.
   */
  has(name: string): boolean {
    return this.tools.has(name);
  }

  /**
   * Check if a tool is enabled.
   * Returns true if tool exists and is not disabled.
   */
  isEnabled(name: string): boolean {
    return this.tools.has(name) && this.enabledTools.has(name);
  }

  /**
   * Enable a tool by name.
   */
  enable(name: string): void {
    if (this.tools.has(name)) {
      this.enabledTools.add(name);
    }
  }

  /**
   * Disable a tool by name.
   */
  disable(name: string): void {
    this.enabledTools.delete(name);
  }

  /**
   * Get all registered tool definitions.
   * Returns array suitable for OpenAI API `tools` parameter.
   */
  getDefinitions(): ToolDefinition[] {
    return Array.from(this.tools.values()).map((t) => t.definition);
  }

  /**
   * Get enabled tool definitions only.
   * Returns array of definitions for tools that are not disabled.
   */
  getEnabledDefinitions(): ToolDefinition[] {
    return Array.from(this.tools.entries())
      .filter(([name]) => this.enabledTools.has(name))
      .map(([, t]) => t.definition);
  }

  /**
   * Get the registered renderer for a tool, if one was provided.
   * Returns undefined for tools without a renderer or unknown tool names.
   */
  getRenderer(toolName: string): ToolResultRenderer | undefined {
    return this.tools.get(toolName)?.renderer;
  }

  /**
   * Get the number of registered tools.
   */
  get size(): number {
    return this.tools.size;
  }

  /**
   * Get all registered tool names.
   */
  getToolNames(): string[] {
    return Array.from(this.tools.keys());
  }

  /**
   * Clear all registered tools and the reverse name map.
   */
  clear(): void {
    this.tools.clear();
    this.enabledTools.clear();
    this._nameMap.clear();
  }

  /**
   * Unregister all tools from a specific source.
   * Useful for removing all tools when an MCP server disconnects.
   * Also removes any reverse name-map entries owned by the same server.
   * @param source - Source identifier (e.g., 'mcp:server-1')
   * @returns Number of tools removed
   */
  unregisterBySource(source: ToolSource): number {
    let count = 0;
    for (const [name, tool] of this.tools) {
      if (tool.source === source) {
        this.tools.delete(name);
        this._nameMap.delete(name); // clean up any name-mapping for this key
        count++;
      }
    }
    return count;
  }

  /**
   * Get the source of a registered tool.
   * @returns Source identifier or undefined if tool not found
   */
  getSource(name: string): ToolSource | undefined {
    return this.tools.get(name)?.source;
  }

  /**
   * Get all tools from a specific source.
   * @param source - Source identifier
   * @returns Array of tool definitions from that source
   */
  getBySource(source: ToolSource): ToolDefinition[] {
    return Array.from(this.tools.values())
      .filter((t) => t.source === source)
      .map((t) => t.definition);
  }

  /**
   * Get a map of sources to their tool counts.
   * Useful for displaying tool statistics in the UI.
   */
  getSourceStats(): Map<ToolSource, number> {
    const stats = new Map<ToolSource, number>();
    for (const tool of this.tools.values()) {
      stats.set(tool.source, (stats.get(tool.source) || 0) + 1);
    }
    return stats;
  }

  /**
   * Get enabled tool definitions grouped by source.
   * Returns a map of source -> definitions for UI grouping.
   */
  getEnabledDefinitionsBySource(): Map<ToolSource, ToolDefinition[]> {
    const grouped = new Map<ToolSource, ToolDefinition[]>();
    for (const [name, tool] of this.tools) {
      if (this.enabledTools.has(name)) {
        const list = grouped.get(tool.source) || [];
        list.push(tool.definition);
        grouped.set(tool.source, list);
      }
    }
    return grouped;
  }
}

// =============================================================================
// Singleton Instance
// =============================================================================

let globalRegistry: ToolRegistry | null = null;

/**
 * Get the global tool registry singleton.
 * Creates it on first access.
 */
export function getToolRegistry(): ToolRegistry {
  if (!globalRegistry) {
    globalRegistry = new ToolRegistry();
  }
  return globalRegistry;
}

/**
 * Reset the global registry (mainly for testing).
 */
export function resetToolRegistry(): void {
  globalRegistry?.clear();
  globalRegistry = null;
}
