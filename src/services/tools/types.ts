/**
 * Tool calling types for the gglib tool registry.
 * These types match the Rust proxy/models.rs types for OpenAI API compatibility.
 */

import type { ReactNode } from 'react';

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
// Tool Rendering Types
// =============================================================================

/**
 * Interface for rendering a tool result in the chat UI.
 * Implementors are plain objects, not React components.
 */
export interface ToolResultRenderer {
  /** Render the full result as a React node for display in the chat. */
  renderResult(data: unknown, toolName: string): ReactNode;
  /** Render a compact plain-text summary (used in collapsed headers). Must not throw. */
  renderSummary?(data: unknown, toolName: string): string;
}

/**
 * A registered tool: its definition, and how its result is drawn.
 */
export interface RegisteredTool {
  /** Tool definition for the LLM */
  definition: ToolDefinition;
  /** Optional renderer for displaying results in the chat UI */
  renderer?: ToolResultRenderer;
}
