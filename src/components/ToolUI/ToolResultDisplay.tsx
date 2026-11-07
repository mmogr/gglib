import React from 'react';
import type { ToolResultRenderer } from '../../services/tools/types';
import { getToolRegistry } from '../../services/tools/registry';
import { fallbackRenderer } from '../../services/tools/renderers';

export interface ToolResultDisplayProps {
  toolName: string;
  result: unknown;
}

/** A tool's result drawn by its registered renderer, by the generic one where it has none or that one throws. */
function renderResult(toolName: string, result: unknown): React.ReactNode {
  const renderer: ToolResultRenderer = getToolRegistry().getRenderer(toolName) ?? fallbackRenderer;
  try {
    return renderer.renderResult(result, toolName);
  } catch {
    return fallbackRenderer.renderResult(result, toolName);
  }
}

/** A tool's result as text to copy: its JSON, or the value itself where it has none. */
export function resultText(result: unknown): string {
  try {
    return JSON.stringify(result, null, 2) ?? String(result);
  } catch {
    return String(result);
  }
}

/**
 * A tool call's result: the renderer the registry holds for its tool, or
 * the generic one, which draws JSON, Markdown or a sortable table by the
 * result's shape (every MCP result goes through it). The one place a result
 * is drawn; the call's row in a reply's fold holds it.
 */
export const ToolResultDisplay: React.FC<ToolResultDisplayProps> = ({ toolName, result }) => {
  // React owns the memoization. If the parent passes unstable result references, it will re-render.
  const body = React.useMemo(() => renderResult(toolName, result), [toolName, result]);
  return <div className="overflow-x-auto max-h-[400px] overflow-y-auto text-xs">{body}</div>;
};
