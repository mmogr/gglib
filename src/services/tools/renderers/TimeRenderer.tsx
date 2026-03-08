import type { ToolResultRenderer } from '../types';
import { fallbackRenderer } from './FallbackRenderer';

interface TimeResult {
  time: string | number;
  timezone: string;
  format: string;
}

function isTimeResult(data: unknown): data is TimeResult {
  const obj: unknown = typeof data === 'string' ? (() => {
    try { return JSON.parse(data); } catch { return data; }
  })() : data;

  return (
    typeof obj === 'object' &&
    obj !== null &&
    'time' in obj &&
    !('error' in obj)
  );
}

/**
 * Renderer for the get_current_time built-in tool.
 * Displays a centered time value with timezone and format metadata.
 * Delegates to fallbackRenderer for any unexpected result shape.
 *
 * Handles both object input and raw JSON-string input (as returned by the
 * backend `ToolResult.content` field).
 */
export const timeRenderer: ToolResultRenderer = {
  renderResult(data, toolName) {
    const resolved: unknown = typeof data === 'string' ? (() => {
      try { return JSON.parse(data); } catch { return data; }
    })() : data;

    if (!isTimeResult(resolved)) {
      return fallbackRenderer.renderResult(resolved, toolName);
    }

    return (
      <div className="text-center py-3">
        <div className="text-lg font-semibold text-text mb-2 font-mono">
          {typeof resolved.time === 'number' ? resolved.time.toString() : resolved.time}
        </div>
        <div className="flex justify-center gap-4 text-[11px] text-text-muted">
          <span>Timezone: {resolved.timezone}</span>
          <span>Format: {resolved.format}</span>
        </div>
      </div>
    );
  },

  renderSummary(data) {
    const resolved: unknown = typeof data === 'string' ? (() => {
      try { return JSON.parse(data); } catch { return data; }
    })() : data;
    try {
      const time = (resolved as Record<string, unknown>)?.time;
      return time !== undefined ? String(time) : '(unknown)';
    } catch {
      return '(unknown)';
    }
  },
};
