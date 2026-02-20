/**
 * Generic Tool UI component for rendering tool calls in the chat.
 * Uses assistant-ui's makeAssistantToolUI to create custom tool renderers.
 */

import React from 'react';
import { makeAssistantToolUI } from '@assistant-ui/react';
import {
  AlertTriangle,
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  Clock3,
  Loader2,
  Wrench,
  XCircle,
} from 'lucide-react';
import { Icon } from '../ui/Icon';
import { cn } from '../../utils/cn';

/**
 * Status indicator component
 */
const StatusBadge: React.FC<{
  status: 'running' | 'complete' | 'error' | 'incomplete';
}> = ({ status }) => {
  const statusConfig = {
    running: { icon: Loader2, label: 'Running', className: 'bg-[rgba(59,130,246,0.2)] text-[#60a5fa]' },
    complete: { icon: CheckCircle2, label: 'Complete', className: 'bg-[rgba(34,197,94,0.2)] text-[#4ade80]' },
    error: { icon: XCircle, label: 'Error', className: 'bg-[rgba(239,68,68,0.2)] text-[#f87171]' },
    incomplete: { icon: AlertTriangle, label: 'Incomplete', className: 'bg-[rgba(234,179,8,0.2)] text-[#facc15]' },
  };

  const config = statusConfig[status];

  return (
    <span className={cn('inline-flex items-center gap-1 px-2 py-0.5 rounded-xl text-[11px] font-medium', config.className)}>
      <span className="text-[10px]" aria-hidden>
        <Icon icon={config.icon} size={14} />
      </span>
      <span className="uppercase tracking-[0.5px]">{config.label}</span>
    </span>
  );
};

/**
 * Collapsible JSON viewer component
 */
const JsonViewer: React.FC<{
  data: unknown;
  label: string;
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

  // For simple values, show inline
  const isSimple = typeof data !== 'object' || data === null;
  if (isSimple) {
    return (
      <div className="mb-2 last:mb-0">
        <span className="font-medium text-text-secondary mr-2">{label}:</span>
        <span className="text-text font-mono">{String(data)}</span>
      </div>
    );
  }

  return (
    <div className="mb-2 last:mb-0">
      <button
        className="flex items-center gap-1.5 bg-transparent border-none py-1 cursor-pointer text-text-secondary text-[13px] text-left w-full hover:text-text"
        onClick={() => setExpanded(!expanded)}
        aria-expanded={expanded}
      >
        <span className="text-[10px] w-3 text-center" aria-hidden>
          <Icon icon={expanded ? ChevronDown : ChevronRight} size={14} />
        </span>
        <span className="font-medium text-text-secondary mr-2">{label}</span>
        {!expanded && (
          <span className="font-mono text-[11px] text-text-muted overflow-hidden text-ellipsis whitespace-nowrap flex-1">
            {formattedJson.length > 50
              ? formattedJson.substring(0, 50) + '...'
              : formattedJson}
          </span>
        )}
      </button>
      {expanded && (
        <pre className="bg-background rounded-sm px-3 py-2 mt-1.5 overflow-x-auto font-mono text-xs text-text max-h-[200px] overflow-y-auto">{formattedJson}</pre>
      )}
    </div>
  );
};

/**
 * Generic tool UI that handles any tool.
 * Shows tool name, arguments, status, and result.
 */
export const GenericToolUI = makeAssistantToolUI<
  Record<string, unknown>,
  unknown
>({
  toolName: '*', // Matches any tool
  render: ({ toolName, args, status, result }) => {
    // Determine display status
    let displayStatus: 'running' | 'complete' | 'error' | 'incomplete' = 'running';
    if (status.type === 'complete') {
      // Check if result indicates an error
      const hasError = result && typeof result === 'object' && 'error' in result;
      displayStatus = hasError ? 'error' : 'complete';
    } else if (status.type === 'incomplete') {
      displayStatus = status.reason === 'error' ? 'error' : 'incomplete';
    }

    // Format tool name for display (e.g., get_current_time -> Get Current Time)
    const displayName = toolName
      .split('_')
      .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
      .join(' ');

    return (
      <div className="bg-background-secondary border border-border rounded-lg my-2 overflow-hidden text-[13px]">
        <div className="flex items-center gap-2 px-3 py-2.5 bg-background-tertiary border-b border-border">
          <span className="text-base" aria-hidden>
            <Icon icon={Wrench} size={14} />
          </span>
          <span className="font-semibold text-text flex-1">{displayName}</span>
          <StatusBadge status={displayStatus} />
        </div>

        <div className="p-3">
          {/* Show arguments */}
          {args && Object.keys(args).length > 0 && (
            <JsonViewer data={args} label="Arguments" defaultExpanded={false} />
          )}

          {/* Show result when complete */}
          {status.type === 'complete' && result !== undefined && (
            <JsonViewer
              data={result}
              label="Result"
              defaultExpanded={true}
            />
          )}

          {/* Show spinner when running */}
          {status.type === 'running' && (
            <div className="flex items-center gap-2 py-2 text-text-secondary">
              <span className="w-4 h-4 border-2 border-border border-t-primary rounded-full animate-spin-360"></span>
              <span>Executing...</span>
            </div>
          )}

          {/* Show error for incomplete with error */}
          {status.type === 'incomplete' && status.reason === 'error' && (
            <div className="px-3 py-2 bg-[rgba(239,68,68,0.1)] rounded-sm text-[#f87171] text-xs">
              Tool execution was interrupted or failed.
            </div>
          )}
        </div>
      </div>
    );
  },
});

/**
 * Specialized tool UI for time-related results.
 * Shows a formatted clock display.
 */
export const TimeToolUI = makeAssistantToolUI<
  { timezone?: string; format?: string },
  { time: string | number; timezone: string; format: string }
>({
  toolName: 'get_current_time',
  render: ({ args, status, result }) => {
    const displayName = 'Get Current Time';

    let displayStatus: 'running' | 'complete' | 'error' | 'incomplete' = 'running';
    if (status.type === 'complete') {
      const hasError = result && typeof result === 'object' && 'error' in result;
      displayStatus = hasError ? 'error' : 'complete';
    } else if (status.type === 'incomplete') {
      displayStatus = status.reason === 'error' ? 'error' : 'incomplete';
    }

    return (
      <div className="bg-background-secondary border border-border rounded-lg my-2 overflow-hidden text-[13px]">
        <div className="flex items-center gap-2 px-3 py-2.5 bg-background-tertiary border-b border-border">
          <span className="text-base" aria-hidden>
            <Icon icon={Clock3} size={14} />
          </span>
          <span className="font-semibold text-text flex-1">{displayName}</span>
          <StatusBadge status={displayStatus} />
        </div>

        <div className="p-3">
          {/* Show timezone argument if provided */}
          {args?.timezone && (
            <div className="flex items-center gap-2 mb-2">
              <span className="font-medium text-text-secondary">Timezone:</span>
              <span className="text-text font-mono">{args.timezone}</span>
            </div>
          )}

          {/* Show formatted time when complete */}
          {status.type === 'complete' && result && !('error' in result) && (
            <div className="text-center py-3">
              <div className="text-lg font-semibold text-text mb-2 font-mono">
                {typeof result.time === 'number'
                  ? result.time.toString()
                  : result.time}
              </div>
              <div className="flex justify-center gap-4 text-[11px] text-text-muted">
                <span>Timezone: {result.timezone}</span>
                <span>Format: {result.format}</span>
              </div>
            </div>
          )}

          {/* Show error */}
          {status.type === 'complete' && result && 'error' in result && (
            <div className="px-3 py-2 bg-[rgba(239,68,68,0.1)] rounded-sm text-[#f87171] text-xs">
              {(result as { error: string }).error}
            </div>
          )}

          {/* Show spinner when running */}
          {status.type === 'running' && (
            <div className="flex items-center gap-2 py-2 text-text-secondary">
              <span className="w-4 h-4 border-2 border-border border-t-primary rounded-full animate-spin-360"></span>
              <span>Fetching time...</span>
            </div>
          )}
        </div>
      </div>
    );
  },
});

export default GenericToolUI;
