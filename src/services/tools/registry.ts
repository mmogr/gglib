/**
 * Tool registry for managing and executing tools.
 * Provides a centralized registry for tool definitions and executors.
 */

import type {
  ToolDefinition,
  ToolExecutor,
  ToolResult,
  RegisteredTool,
  ParsedToolCall,
} from './types';
import { parseToolCall, ToolCall } from './types';

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
 * Registry for managing tools available to the LLM.
 * Handles tool registration, lookup, and execution.
 */
export class ToolRegistry {
  private tools = new Map<string, RegisteredToolWithSource>();
  // Secure-by-default: tools are disabled unless explicitly enabled.
  // We keep an allowlist instead of a denylist so registration cannot
  // accidentally re-enable tools (e.g., during MCP resync).
  private enabledTools = new Set<string>();

  /**
   * Register a tool with its definition and executor.
   * @param definition - OpenAI-compatible tool definition
   * @param execute - Function to execute when tool is called
   * @param source - Source identifier for the tool (default: 'builtin')
   * @throws Error if tool with same name already exists
   */
  register(definition: ToolDefinition, execute: ToolExecutor, source: ToolSource = 'builtin'): void {
    const name = definition.function.name;
    if (this.tools.has(name)) {
      throw new Error(`Tool "${name}" is already registered`);
    }
    this.tools.set(name, { definition, execute, source });
    // Newly registered tools are disabled by default.
    // Intentionally do not mutate enable-state here so that if a tool is
    // re-registered after being enabled (e.g., MCP resync), it stays enabled.
  }

  /**
   * Register a tool using a simplified builder pattern.
   * @param name - Function name
   * @param description - Description for the LLM
   * @param parameters - JSON Schema for parameters (optional)
   * @param execute - Executor function
   * @param source - Source identifier for the tool (default: 'builtin')
   */
  registerFunction(
    name: string,
    description: string,
    parameters: ToolDefinition['function']['parameters'] | undefined,
    execute: ToolExecutor,
    source: ToolSource = 'builtin'
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
      execute,
      source
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
   * Get a specific tool's executor.
   */
  getExecutor(name: string): ToolExecutor | undefined {
    return this.tools.get(name)?.execute;
  }

  /**
   * Execute a tool by name with given arguments.
   * Handles errors gracefully, returning error result instead of throwing.
   */
  async execute(
    name: string,
    args: Record<string, unknown>
  ): Promise<ToolResult> {
    const tool = this.tools.get(name);
    if (!tool) {
      return { success: false, error: `Unknown tool: ${name}` };
    }

    try {
      const result = await tool.execute(args);
      return result;
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      return { success: false, error: message };
    }
  }

  /**
   * Execute a parsed tool call.
   * Convenience method that takes a ParsedToolCall directly.
   */
  async executeCall(call: ParsedToolCall): Promise<ToolResult> {
    return this.execute(call.name, call.arguments);
  }

  /**
   * Execute a raw ToolCall from the LLM.
   * Parses arguments JSON and executes.
   */
  async executeRawCall(toolCall: ToolCall): Promise<ToolResult> {
    const parsed = parseToolCall(toolCall);
    if (!parsed) {
      return {
        success: false,
        error: `Failed to parse arguments for tool call ${toolCall.id}`,
      };
    }
    return this.executeCall(parsed);
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
   * Clear all registered tools.
   */
  clear(): void {
    this.tools.clear();
    this.enabledTools.clear();
  }

  /**
   * Unregister all tools from a specific source.
   * Useful for removing all tools when an MCP server disconnects.
   * @param source - Source identifier (e.g., 'mcp:server-1')
   * @returns Number of tools removed
   */
  unregisterBySource(source: ToolSource): number {
    let count = 0;
    for (const [name, tool] of this.tools) {
      if (tool.source === source) {
        this.tools.delete(name);
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
