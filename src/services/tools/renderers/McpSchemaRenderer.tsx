import { SchemaBasedView } from './SchemaBasedView';
import { fallbackRenderer } from './FallbackRenderer';
import type { ToolResultRenderer } from '../types';

/**
 * Factory: given a tool's output JSON Schema, returns a ToolResultRenderer
 * that uses SchemaBasedView for structured display.
 *
 * Used when an MCP server declares an output_schema for a tool.
 * Falls back to fallbackRenderer.renderSummary for the summary line.
 */
export const createMcpSchemaRenderer = (
  schema: Record<string, unknown>,
): ToolResultRenderer => ({
  renderResult(data) {
    return <SchemaBasedView data={data} schema={schema} />;
  },
  renderSummary(data, toolName) {
    return fallbackRenderer.renderSummary!(data, toolName);
  },
});
