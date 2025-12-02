/**
 * Tool calling types for the gglib tool registry.
 * These types match the Rust proxy/models.rs types for OpenAI API compatibility.
 */

// =============================================================================
// Tool Definition Types (for declaring tools)
// =============================================================================

/**
 * JSON Schema type for function parameters.
 * Simplified subset of JSON Schema for tool parameter definitions.
 */
export interface JSONSchema {
  type: 'object' | 'string' | 'number' | 'boolean' | 'array';
  properties?: Record<string, JSONSchemaProperty>;
  required?: string[];
  description?: string;
  items?: JSONSchemaProperty;
}

export interface JSONSchemaProperty {
  type: 'string' | 'number' | 'boolean' | 'array' | 'object';
  description?: string;
  enum?: (string | number)[];
  items?: JSONSchemaProperty;
  properties?: Record<string, JSONSchemaProperty>;
  required?: string[];
  default?: unknown;
}

/**
 * Function definition within a tool (OpenAI-compatible).
 * Matches Rust FunctionDefinition.
 */
export interface FunctionDefinition {
  /** Function name - should be lowercase with underscores */
  name: string;
  /** Description of what the function does (shown to LLM) */
  description?: string;
  /** JSON Schema for function parameters */
  parameters?: JSONSchema;
}

/**
 * Tool definition for function calling (OpenAI-compatible).
 * Matches Rust ToolDefinition.
 */
export interface ToolDefinition {
  /** Tool type - always "function" */
  type: 'function';
  /** Function definition */
  function: FunctionDefinition;
}

// =============================================================================
// Tool Execution Types
// =============================================================================

/**
 * Result from executing a tool.
 * Uses discriminated union for type-safe success/error handling.
 */
export type ToolResult =
  | { success: true; data: unknown }
  | { success: false; error: string };

/**
 * Function signature for tool executors.
 * Takes parsed arguments, returns a ToolResult.
 */
export type ToolExecutor = (
  args: Record<string, unknown>
) => Promise<ToolResult> | ToolResult;

/**
 * A registered tool with its definition and executor.
 */
export interface RegisteredTool {
  /** Tool definition for the LLM */
  definition: ToolDefinition;
  /** Function to execute when the tool is called */
  execute: ToolExecutor;
}

// =============================================================================
// Tool Call Types (from LLM responses)
// =============================================================================

/**
 * Function call details within a tool call.
 * Matches Rust ToolCallFunction.
 */
export interface ToolCallFunction {
  /** Name of the function to call */
  name: string;
  /** JSON string of arguments */
  arguments: string;
}

/**
 * A complete tool call from the assistant.
 * Matches Rust ToolCall.
 */
export interface ToolCall {
  /** Unique ID for this tool call */
  id: string;
  /** Tool type - always "function" */
  type: 'function';
  /** Function call details */
  function: ToolCallFunction;
}

/**
 * Parsed tool call with arguments as object.
 * Convenience type for after JSON parsing.
 */
export interface ParsedToolCall {
  id: string;
  name: string;
  arguments: Record<string, unknown>;
}

/**
 * Parse a ToolCall into a ParsedToolCall.
 * Returns null if arguments JSON is invalid.
 */
export function parseToolCall(toolCall: ToolCall): ParsedToolCall | null {
  try {
    const args = JSON.parse(toolCall.function.arguments);
    return {
      id: toolCall.id,
      name: toolCall.function.name,
      arguments: typeof args === 'object' && args !== null ? args : {},
    };
  } catch {
    return null;
  }
}
