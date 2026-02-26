/**
 * Tool execution progress events.
 *
 * Emitted by executeToolBatch as each individual tool starts and settles,
 * providing a unified event stream for UI feedback and telemetry.
 *
 * @module types/events/toolExecution
 */

/** Fired immediately before a tool's function is invoked. */
export interface ToolStartEvent {
  type: 'tool-start';
  toolCallId: string;
  toolName: string;
  timestamp: number;
}

/** Fired when a tool returns a successful result. */
export interface ToolCompleteEvent {
  type: 'tool-complete';
  toolCallId: string;
  toolName: string;
  timestamp: number;
  /** Stringified result payload. */
  result: string;
  /** Elapsed wall-clock time measured with performance.now(), in milliseconds. */
  durationMs: number;
}

/** Fired when a tool fails (execution error or timeout). */
export interface ToolErrorEvent {
  type: 'tool-error';
  toolCallId: string;
  toolName: string;
  timestamp: number;
  /** Human-readable error message. */
  error: string;
  /** Elapsed wall-clock time measured with performance.now(), in milliseconds. */
  durationMs: number;
}

/** Union of all tool execution lifecycle events. */
export type ToolExecutionEvent = ToolStartEvent | ToolCompleteEvent | ToolErrorEvent;

/** Callback type for receiving tool execution lifecycle events. */
export type OnToolEvent = (event: ToolExecutionEvent) => void;
