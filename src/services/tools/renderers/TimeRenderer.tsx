import type { ToolResultRenderer } from '../types';
import { fallbackRenderer } from './FallbackRenderer';

interface TimeResult {
  time: string | number;
  timezone: string;
  format: string;
}

function isTimeResult(data: unknown): data is TimeResult {
  return (
    typeof data === 'object' &&
    data !== null &&
    'time' in data &&
    !('error' in data)
  );
}

/**
 * Renderer for the get_current_time built-in tool.
 * Displays a centered time value with timezone and format metadata.
 * Delegates to fallbackRenderer for any unexpected result shape.
 */
export const timeRenderer: ToolResultRenderer = {
  renderResult(data, toolName) {
    if (!isTimeResult(data)) {
      return fallbackRenderer.renderResult(data, toolName);
    }

    return (
      <div className="text-center py-3">
        <div className="text-lg font-semibold text-text mb-2 font-mono">
          {typeof data.time === 'number' ? data.time.toString() : data.time}
        </div>
        <div className="flex justify-center gap-4 text-[11px] text-text-muted">
          <span>Timezone: {data.timezone}</span>
          <span>Format: {data.format}</span>
        </div>
      </div>
    );
  },

  renderSummary(data) {
    try {
      const time = (data as Record<string, unknown>)?.time;
      return time !== undefined ? String(time) : '(unknown)';
    } catch {
      return '(unknown)';
    }
  },
};
