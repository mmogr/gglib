import React from 'react';
import { ChevronDown, ChevronRight } from 'lucide-react';
import { Button } from '../../../components/ui/Button';
import type { ToolResultRenderer } from '../types';

const SUMMARY_MAX_LENGTH = 80;

/**
 * Collapsible JSON viewer for arbitrary data.
 */
export const JsonViewer: React.FC<{
  data: unknown;
  label?: string;
  defaultExpanded?: boolean;
}> = ({ data, label, defaultExpanded = false }) => {
  const [expanded, setExpanded] = React.useState(defaultExpanded);

  const formattedJson = React.useMemo(() => {
    try {
      return JSON.stringify(data, null, 2);
    } catch {
      return String(data);
    }
  }, [data]);

  // Primitives and null are shown inline — no collapse needed.
  const isSimple = typeof data !== 'object' || data === null;
  if (isSimple) {
    return (
      <div className="mb-2 last:mb-0">
        {label && <span className="font-medium text-text-secondary mr-2">{label}:</span>}
        <span className="text-text font-mono text-xs">{String(data)}</span>
      </div>
    );
  }

  return (
    <div className="mb-2 last:mb-0">
      <Button
        variant="ghost"
        size="sm"
        className="w-full justify-start gap-1.5 px-0 hover:bg-transparent"
        onClick={() => setExpanded(!expanded)}
        aria-expanded={expanded}
      >
        <span className="text-2xs w-3 text-center" aria-hidden>
          {expanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
        </span>
        {label && <span className="font-medium text-text-secondary mr-2">{label}</span>}
        {!expanded && (
          <span className="font-mono text-2xs text-text-muted overflow-hidden text-ellipsis whitespace-nowrap flex-1">
            {formattedJson.length > 50 ? `${formattedJson.substring(0, 50)}...` : formattedJson}
          </span>
        )}
      </Button>
      {expanded && (
        <pre className="bg-background rounded-sm px-3 py-2 mt-1.5 overflow-x-auto font-mono text-xs text-text max-h-[200px] overflow-y-auto">
          {formattedJson}
        </pre>
      )}
    </div>
  );
};

function safeJsonSummary(data: unknown): string {
  try {
    const raw = JSON.stringify(data);
    if (raw === undefined) return '(result)';
    return raw.length > SUMMARY_MAX_LENGTH ? `${raw.substring(0, SUMMARY_MAX_LENGTH)}…` : raw;
  } catch {
    return '(result)';
  }
}

/**
 * Fallback renderer: renders any tool result as pretty-printed JSON.
 * Used for all tools that don't register a custom renderer.
 */
export const fallbackRenderer: ToolResultRenderer = {
  renderResult(data) {
    return <JsonViewer data={data} defaultExpanded />;
  },
  renderSummary(data) {
    return safeJsonSummary(data);
  },
};
