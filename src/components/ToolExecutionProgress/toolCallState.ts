/**
 * A reply's tool calls as its fold lists them: one per tool-call part, each
 * running, done or failed, read from the part as the run reader stamps it
 * (`agentMessageState`) and as a saved row is loaded (`buildLoadedMessage`),
 * so a call reloaded says what it said live.
 *
 * @module toolCallState
 */

import { getToolRegistry } from '../../services/tools/registry';
import { formatToolDisplayName } from '../../services/tools/nameUtils';
import { toolCallProgress, type ToolProgress } from '../../types/messages';

/** What a tool-call part carries, the run reader's stamps among it. */
interface ToolCallPartLike {
  type: 'tool-call';
  toolCallId: string;
  toolName: string;
  args?: unknown;
  result?: unknown;
  /** Elapsed wall-clock time, once the tool has settled. */
  durationMs?: number;
  /** The result is the tool's failure. */
  isError?: boolean;
  /** The name the daemon shows the tool by (e.g. "Read File"). */
  displayName?: string;
  /** The MCP server the tool belongs to; absent for gglib's own tools. */
  server?: string;
  progress?: unknown;
}

export type ToolCallState = 'running' | 'done' | 'failed';

export interface ToolCallRow {
  id: string;
  /** The tool's name as the model called it. */
  toolName: string;
  /** The name it is shown by. */
  label: string;
  server?: string;
  state: ToolCallState;
  durationMs?: number;
  /** The first 80 characters of a failure's message. */
  error?: string;
  /** How far a running tool has got, when it says. */
  progress?: ToolProgress;
  args: unknown;
  /** Absent while it runs. */
  result?: unknown;
}

/**
 * The name a tool is shown by: the daemon's, or the registry's original
 * name for one it sanitised (dots, spaces), formatted as the daemon would.
 */
function labelOf(part: ToolCallPartLike): string {
  if (part.displayName) return part.displayName;
  const raw = getToolRegistry().getOriginalName(part.toolName) ?? part.toolName;
  return formatToolDisplayName(raw);
}

function errorOf(result: unknown): string {
  const said = typeof result === 'object' && result !== null
    ? ((result as { error?: string }).error ?? String(result))
    : String(result ?? '');
  return said.slice(0, 80);
}

function isToolCall(part: unknown): part is ToolCallPartLike {
  return typeof part === 'object' && part !== null && (part as { type?: unknown }).type === 'tool-call';
}

/**
 * What a screen reader is told of a reply's tool calls as they settle: one
 * sentence per call that has, in the order called, each keyed by its call so
 * a live region adds only the new one and never reads the others again.
 */
export function settledWords(rows: readonly ToolCallRow[]): { id: string; words: string }[] {
  return rows
    .filter((row) => row.state !== 'running')
    .map((row) => ({ id: row.id, words: row.state === 'failed' ? `${row.label} failed.` : `${row.label} done.` }));
}

/** One row per tool call of a reply's content, in the order called. */
export function toolCallRows(content: readonly unknown[]): ToolCallRow[] {
  return content.filter(isToolCall).map((part) => {
    const row: ToolCallRow = {
      id: part.toolCallId,
      toolName: part.toolName,
      label: labelOf(part),
      ...(part.server && { server: part.server }),
      state: 'running',
      args: part.args,
    };
    if (!('result' in part)) return { ...row, progress: toolCallProgress(part) };
    const settled = { ...row, result: part.result, ...(part.durationMs !== undefined && { durationMs: part.durationMs }) };
    return part.isError === true
      ? { ...settled, state: 'failed', error: errorOf(part.result) }
      : { ...settled, state: 'done' };
  });
}
