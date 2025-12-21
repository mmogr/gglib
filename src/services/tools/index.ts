/**
 * Tool registry module.
 * Provides a centralized registry for tools that can be called by the LLM.
 *
 * @example
 * ```typescript
 * import { getToolRegistry, ToolDefinition } from '../services/tools';
 *
 * // Get tool definitions for LLM request
 * const registry = getToolRegistry();
 * const tools = registry.getDefinitions();
 *
 * // Execute a tool call from LLM response
 * const result = await registry.execute('get_current_time', { timezone: 'UTC' });
 * ```
 */

// Re-export types
export type {
  ToolDefinition,
  FunctionDefinition,
  ToolExecutor,
  ToolResult,
  RegisteredTool,
  ToolCall,
  ToolCallFunction,
  ParsedToolCall,
  JSONSchema,
  JSONSchemaProperty,
} from './types';

export { parseToolCall } from './types';

// Re-export registry
export {
  ToolRegistry,
  getToolRegistry,
  resetToolRegistry,
  type ToolSource,
} from './registry';

// Re-export MCP integration
export {
  registerMcpTools,
  unregisterMcpTools,
  syncAllMcpTools,
  getMcpToolsByServer,
  getMcpSource,
  onMcpToolsChange,
  notifyMcpToolsChanged,
} from './mcpIntegration';

// Register built-in tools on import
import { registerBuiltinTools } from './builtin';
registerBuiltinTools();

// Re-export builtin tools for direct access
export { registerBuiltinTools } from './builtin';
