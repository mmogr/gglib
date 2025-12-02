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
 * Registry for managing tools available to the LLM.
 * Handles tool registration, lookup, and execution.
 */
export class ToolRegistry {
  private tools = new Map<string, RegisteredTool>();
  private disabledTools = new Set<string>();

  /**
   * Register a tool with its definition and executor.
   * @param definition - OpenAI-compatible tool definition
   * @param execute - Function to execute when tool is called
   * @throws Error if tool with same name already exists
   */
  register(definition: ToolDefinition, execute: ToolExecutor): void {
    const name = definition.function.name;
    if (this.tools.has(name)) {
      throw new Error(`Tool "${name}" is already registered`);
    }
    this.tools.set(name, { definition, execute });
    // Newly registered tools are enabled by default
    this.disabledTools.delete(name);
  }

  /**
   * Register a tool using a simplified builder pattern.
   * @param name - Function name
   * @param description - Description for the LLM
   * @param parameters - JSON Schema for parameters (optional)
   * @param execute - Executor function
   */
  registerFunction(
    name: string,
    description: string,
    parameters: ToolDefinition['function']['parameters'] | undefined,
    execute: ToolExecutor
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
      execute
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
    return this.tools.has(name) && !this.disabledTools.has(name);
  }

  /**
   * Enable a tool by name.
   */
  enable(name: string): void {
    this.disabledTools.delete(name);
  }

  /**
   * Disable a tool by name.
   */
  disable(name: string): void {
    if (this.tools.has(name)) {
      this.disabledTools.add(name);
    }
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
      .filter(([name]) => !this.disabledTools.has(name))
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
    this.disabledTools.clear();
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
