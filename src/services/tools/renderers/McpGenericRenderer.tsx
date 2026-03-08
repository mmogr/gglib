import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { SortableTable } from '../../../components/ToolUI/SortableTable';
import { fallbackRenderer } from './FallbackRenderer';
import type { ToolResultRenderer } from '../types';

// ---------------------------------------------------------------------------
// Heuristic helpers
// ---------------------------------------------------------------------------

const MARKDOWN_PATTERNS = [
  /^#{1,6} /m,         // ATX headings
  /\*\*[^*]+\*\*/,     // bold
  /^- /m,              // unordered list item
  /`[^`]+`/,           // inline code or fenced block
  /\[[^\]]+\]\([^)]+\)/, // link
];

/**
 * Returns true if the string is likely Markdown prose.
 * Requires length > 20 AND at least 2 matching Markdown patterns to reduce
 * false positives on short strings that happen to contain a lone `*` or `#`.
 */
export function looksLikeMarkdown(s: string): boolean {
  if (s.length <= 20) return false;
  let matches = 0;
  for (const pattern of MARKDOWN_PATTERNS) {
    if (pattern.test(s)) {
      matches++;
      if (matches >= 2) return true;
    }
  }
  return false;
}

/**
 * Returns true if data is a non-empty array where every element is an object
 * that shares at least one key with the first element.
 * This identifies homogeneous record arrays suitable for tabular rendering.
 */
export function isArrayOfHomogeneousObjects(
  data: unknown,
): data is Record<string, unknown>[] {
  if (!Array.isArray(data) || data.length === 0) return false;
  const first = data[0];
  if (typeof first !== 'object' || first === null) return false;
  const firstKeys = new Set(Object.keys(first as object));
  if (firstKeys.size === 0) return false;
  return data.every(
    (item) =>
      typeof item === 'object' &&
      item !== null &&
      Object.keys(item as object).some((k) => firstKeys.has(k)),
  );
}

// ---------------------------------------------------------------------------
// Renderer
// ---------------------------------------------------------------------------

/**
 * Heuristic renderer for MCP tools that don't declare an output schema.
 *
 * Dispatch order:
 *  1. String that looks like Markdown → ReactMarkdown
 *  2. Homogeneous array of objects   → SortableTable
 *  3. Everything else                → fallbackRenderer (pretty-printed JSON)
 */
export const mcpGenericRenderer: ToolResultRenderer = {
  renderResult(data, toolName) {
    if (typeof data === 'string' && looksLikeMarkdown(data)) {
      return (
        <div className="prose-sm text-text text-[13px] leading-relaxed [&_a]:text-primary [&_a]:underline [&_code]:bg-background [&_code]:rounded [&_code]:px-1 [&_pre]:bg-background [&_pre]:rounded [&_pre]:p-2 [&_pre]:overflow-x-auto">
          <ReactMarkdown remarkPlugins={[remarkGfm]}>{data}</ReactMarkdown>
        </div>
      );
    }

    if (isArrayOfHomogeneousObjects(data)) {
      return <SortableTable rows={data} />;
    }

    return fallbackRenderer.renderResult(data, toolName);
  },

  renderSummary(data, toolName) {
    return fallbackRenderer.renderSummary!(data, toolName);
  },
};
