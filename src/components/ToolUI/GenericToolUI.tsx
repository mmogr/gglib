/**
 * Generic Tool UI component for rendering tool calls in the chat.
 * Uses assistant-ui's makeAssistantToolUI to create custom tool renderers.
 */

import React from 'react';
import { makeAssistantToolUI } from '@assistant-ui/react';
import styles from './ToolUI.module.css';

/**
 * Status indicator component
 */
const StatusBadge: React.FC<{
  status: 'running' | 'complete' | 'error' | 'incomplete';
}> = ({ status }) => {
  const statusConfig = {
    running: { icon: '⏳', label: 'Running', className: styles.statusRunning },
    complete: { icon: '✅', label: 'Complete', className: styles.statusComplete },
    error: { icon: '❌', label: 'Error', className: styles.statusError },
    incomplete: { icon: '⚠️', label: 'Incomplete', className: styles.statusIncomplete },
  };

  const config = statusConfig[status];

  return (
    <span className={`${styles.statusBadge} ${config.className}`}>
      <span className={styles.statusIcon}>{config.icon}</span>
      <span className={styles.statusLabel}>{config.label}</span>
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
      <div className={styles.jsonViewer}>
        <span className={styles.jsonLabel}>{label}:</span>
        <span className={styles.jsonSimpleValue}>{String(data)}</span>
      </div>
    );
  }

  return (
    <div className={styles.jsonViewer}>
      <button
        className={styles.jsonToggle}
        onClick={() => setExpanded(!expanded)}
        aria-expanded={expanded}
      >
        <span className={styles.jsonToggleIcon}>{expanded ? '▼' : '▶'}</span>
        <span className={styles.jsonLabel}>{label}</span>
        {!expanded && (
          <span className={styles.jsonPreview}>
            {formattedJson.length > 50
              ? formattedJson.substring(0, 50) + '...'
              : formattedJson}
          </span>
        )}
      </button>
      {expanded && (
        <pre className={styles.jsonContent}>{formattedJson}</pre>
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
      <div className={styles.toolCard}>
        <div className={styles.toolHeader}>
          <span className={styles.toolIcon}>🔧</span>
          <span className={styles.toolName}>{displayName}</span>
          <StatusBadge status={displayStatus} />
        </div>

        <div className={styles.toolBody}>
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
            <div className={styles.runningIndicator}>
              <span className={styles.spinner}></span>
              <span>Executing...</span>
            </div>
          )}

          {/* Show error for incomplete with error */}
          {status.type === 'incomplete' && status.reason === 'error' && (
            <div className={styles.errorMessage}>
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
      <div className={styles.toolCard}>
        <div className={styles.toolHeader}>
          <span className={styles.toolIcon}>🕐</span>
          <span className={styles.toolName}>{displayName}</span>
          <StatusBadge status={displayStatus} />
        </div>

        <div className={styles.toolBody}>
          {/* Show timezone argument if provided */}
          {args?.timezone && (
            <div className={styles.timeArgument}>
              <span className={styles.argLabel}>Timezone:</span>
              <span className={styles.argValue}>{args.timezone}</span>
            </div>
          )}

          {/* Show formatted time when complete */}
          {status.type === 'complete' && result && !('error' in result) && (
            <div className={styles.timeResult}>
              <div className={styles.timeDisplay}>
                {typeof result.time === 'number'
                  ? result.time.toString()
                  : result.time}
              </div>
              <div className={styles.timeMetadata}>
                <span>Timezone: {result.timezone}</span>
                <span>Format: {result.format}</span>
              </div>
            </div>
          )}

          {/* Show error */}
          {status.type === 'complete' && result && 'error' in result && (
            <div className={styles.errorMessage}>
              {(result as { error: string }).error}
            </div>
          )}

          {/* Show spinner when running */}
          {status.type === 'running' && (
            <div className={styles.runningIndicator}>
              <span className={styles.spinner}></span>
              <span>Fetching time...</span>
            </div>
          )}
        </div>
      </div>
    );
  },
});

export default GenericToolUI;
