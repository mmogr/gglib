/**
 * Real-time parallel tool execution progress panel.
 *
 * Renders one row per tool-call content part, deriving each tool's status
 * (running / complete / error) reactively from `useMessage()`. Stays mounted
 * as a collapsible accordion after all tools settle, preventing layout shifts.
 *
 * @module ToolExecutionProgress
 */

import React, { useState, useRef, useMemo } from 'react';
import { useMessage } from '@assistant-ui/react';
import type { ThreadMessage } from '@assistant-ui/react';
import { CheckCircle2, ChevronDown, ChevronRight, Loader2, XCircle } from 'lucide-react';
import { cn } from '../../utils/cn';
import { Icon } from '../ui/Icon';
import { getToolRegistry } from '../../services/tools/registry';
import { formatToolDisplayName } from '../../services/tools/nameUtils';

// =============================================================================
// Types
// =============================================================================

type ToolCallPart = Extract<ThreadMessage['content'][number], { type: 'tool-call' }>;

/**
 * Extends the base ToolCallPart with runtime fields stamped by runAgenticLoop
 * as each tool settles. These fields are not part of the @assistant-ui/react
 * type surface because they are added dynamically.
 */
type AugmentedToolCallPart = ToolCallPart & {
  /** Elapsed wall-clock time in milliseconds (present once the tool has settled). */
  durationMs?: number;
  /** True when the tool result represents an error condition. */
  isError?: boolean;
};

type ToolRowState = 'running' | 'complete' | 'error';

interface ToolRowData {
  toolCallId: string;
  toolName: string;
  state: ToolRowState;
  durationMs?: number;
  /** First 80 chars of the error message, for inline display. */
  errorSummary?: string;
}

// =============================================================================
// Helpers
// =============================================================================

/**
 * Resolve a (possibly sanitized) tool registry key to a human-readable label.
 *
 * Looks up the original raw MCP tool name via the reverse name map so that
 * tools with special characters (dots, spaces, unicode) display correctly.
 * Falls back to the sanitized name itself for non-MCP / built-in tools.
 */
function formatToolName(name: string): string {
  const raw = getToolRegistry().getOriginalName(name) ?? name;
  return formatToolDisplayName(raw);
}

/**
 * Format a millisecond duration for compact display.
 */
function formatDuration(ms: number): string {
  return ms < 1000 ? `${Math.round(ms)}ms` : `${(ms / 1000).toFixed(1)}s`;
}

/**
 * Map a tool-call content part to display data.
 * `durationMs` and `isError` are custom fields stamped by runAgenticLoop when each tool settles.
 */
function classifyPart(part: ToolCallPart): ToolRowData {
  const augmented = part as AugmentedToolCallPart;
  const durationMs = augmented.durationMs;

  if (!('result' in part)) {
    return { toolCallId: part.toolCallId, toolName: part.toolName, state: 'running' };
  }

  if (augmented.isError === true) {
    const raw = part.result as { error?: string } | string | null;
    const errorSummary = (typeof raw === 'object' && raw !== null
      ? (raw.error ?? String(raw))
      : String(raw ?? '')
    ).slice(0, 80);
    return {
      toolCallId: part.toolCallId,
      toolName: part.toolName,
      state: 'error',
      durationMs,
      errorSummary,
    };
  }

  return { toolCallId: part.toolCallId, toolName: part.toolName, state: 'complete', durationMs };
}

// =============================================================================
// Sub-components
// =============================================================================

/**
 * A single tool execution row: icon + tool name + optional duration / error.
 */
const ToolRow: React.FC<{ row: ToolRowData }> = ({ row }) => (
  <div
    className={cn(
      'flex items-center gap-2 py-[3px] px-2 rounded-md text-[12px]',
      row.state === 'running' && 'text-[#60a5fa]',
      row.state === 'complete' && 'text-[#4ade80]',
      row.state === 'error' && 'text-[#f87171]',
    )}
  >
    {row.state === 'running' && (
      <Icon icon={Loader2} size={13} className="flex-shrink-0 animate-spin" />
    )}
    {row.state === 'complete' && (
      <Icon icon={CheckCircle2} size={13} className="flex-shrink-0" />
    )}
    {row.state === 'error' && (
      <Icon icon={XCircle} size={13} className="flex-shrink-0" />
    )}

    <span className="font-medium truncate">{formatToolName(row.toolName)}</span>

    {row.state === 'error' && row.errorSummary && (
      <span className="ml-1 text-text-muted truncate max-w-[160px]">
        {row.errorSummary}
      </span>
    )}

    {row.durationMs !== undefined && (
      <span className="ml-auto font-mono text-[11px] text-text-muted flex-shrink-0">
        {formatDuration(row.durationMs)}
      </span>
    )}
  </div>
);

// =============================================================================
// Main component
// =============================================================================

/**
 * Parallel tool execution progress panel.
 *
 * Reads tool-call parts from the current message via `useMessage()`. Each part
 * is classified as `running`, `complete`, or `error` based on whether a
 * `result` field is present (stamped by runAgenticLoop as each tool settles).
 *
 * The accordion defaults to expanded and **never auto-collapses** — only the
 * user can toggle it. This prevents CLS when the last tool finishes.
 */
const ToolExecutionProgress: React.FC = () => {
  const message = useMessage();
  const [isCollapsed, setIsCollapsed] = useState(false);

  // Track which tool IDs have already been announced so we emit only once per
  // completion, not on every re-render.
  const announcedIds = useRef<Set<string>>(new Set());

  // Extract tool-call parts from the live message content.
  const toolParts = useMemo(
    () =>
      (Array.isArray(message.content) ? message.content : []).filter(
        (p): p is ToolCallPart =>
          typeof p !== 'string' && p.type === 'tool-call',
      ),
    [message.content],
  );

  const rows = useMemo(() => toolParts.map(classifyPart), [toolParts]);

  // Build announcements for tools that just transitioned out of 'running'.
  const newAnnouncements = useMemo(() => {
    const out: string[] = [];
    for (const row of rows) {
      if (row.state === 'running') continue;
      if (announcedIds.current.has(row.toolCallId)) continue;
      announcedIds.current.add(row.toolCallId);

      if (row.state === 'complete') {
        const dur = row.durationMs !== undefined ? ` in ${formatDuration(row.durationMs)}` : '';
        out.push(`Tool ${formatToolName(row.toolName)} complete${dur}.`);
      } else {
        const err = row.errorSummary ? `: ${row.errorSummary}` : '';
        out.push(`Tool ${formatToolName(row.toolName)} failed${err}.`);
      }
    }
    return out;
  }, [rows]);

  // Render nothing if the message has no tool calls.
  if (rows.length === 0) return null;

  const runningCount = rows.filter(r => r.state === 'running').length;
  const headerLabel =
    runningCount > 0
      ? `Running ${runningCount} of ${rows.length} tool${rows.length === 1 ? '' : 's'}…`
      : `${rows.length} tool${rows.length === 1 ? '' : 's'} complete`;

  return (
    <div className="mt-2 border border-border rounded-lg overflow-hidden text-sm">
      {/* ── Accordion header ────────────────────────────────────────────── */}
      <button
        type="button"
        className="w-full flex items-center gap-2 px-3 py-2 bg-background-secondary text-[12px] font-medium text-text-secondary hover:bg-background-tertiary transition-colors duration-150 border-none cursor-pointer"
        onClick={() => setIsCollapsed(prev => !prev)}
        aria-expanded={!isCollapsed}
        aria-controls="tool-execution-rows"
      >
        <Icon
          icon={isCollapsed ? ChevronRight : ChevronDown}
          size={13}
          className="flex-shrink-0 transition-transform duration-150"
        />
        <span>{headerLabel}</span>
        {runningCount > 0 && (
          <Icon icon={Loader2} size={12} className="animate-spin ml-1 text-[#60a5fa]" />
        )}
      </button>

      {/* ── Per-tool rows ────────────────────────────────────────────────── */}
      {!isCollapsed && (
        <div
          id="tool-execution-rows"
          role="list"
          aria-label="Tool execution status"
          className="flex flex-col gap-[2px] p-2 bg-background"
        >
          {rows.map(row => (
            <div key={row.toolCallId} role="listitem">
              <ToolRow row={row} />
            </div>
          ))}
        </div>
      )}

      {/* ── Screen reader live region ────────────────────────────────────── */}
      <div
        role="status"
        aria-live="polite"
        aria-atomic="false"
        className="sr-only"
      >
        {newAnnouncements.join(' ')}
      </div>
    </div>
  );
};

export default ToolExecutionProgress;
