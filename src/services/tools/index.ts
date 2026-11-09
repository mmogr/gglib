/**
 * Tool registry module.
 * Provides a centralized registry of the tools the LLM can call. The daemon
 * runs them; the registry holds their definitions, the daemon's names for
 * them and their renderers. Which of them a chat's runs may call is saved
 * with the chat (`hooks/useChatTools`).
 *
 * @example
 * ```typescript
 * import { getToolRegistry } from '../services/tools';
 *
 * // Every registered tool, by the name the daemon knows it by
 * const registry = getToolRegistry();
 * const names = registry.getDefinitions().map((d) => registry.getBackendName(d.function.name));
 * ```
 */

// Re-export types
export type {
  ToolDefinition,
  FunctionDefinition,
  RegisteredTool,
  JSONSchema,
  JSONSchemaProperty,
} from './types';

// Re-export registry
export {
  ToolRegistry,
  getToolRegistry,
  resetToolRegistry,
  type ToolSource,
  type NameMapEntry,
} from './registry';

// Re-export MCP integration
export {
  registerMcpTools,
  unregisterMcpTools,
  syncAllMcpTools,
  getMcpSource,
} from './mcpIntegration';

// Re-export built-in integration
export { syncBuiltinTools } from './builtinIntegration';

// Re-export name utilities
export {
  sanitizeToolName,
  detectCollisions,
  formatToolDisplayName,
} from './nameUtils';
