/**
 * Tool registry module.
 * Provides a centralized registry of the tools the LLM can call. The daemon
 * runs them; the registry holds their definitions, enablement and renderers.
 *
 * @example
 * ```typescript
 * import { getToolRegistry, ToolDefinition } from '../services/tools';
 *
 * // The tools the person has enabled, by the names the daemon knows them by
 * const registry = getToolRegistry();
 * const toolFilter = registry
 *   .getEnabledDefinitions()
 *   .map((d) => registry.getBackendName(d.function.name));
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
