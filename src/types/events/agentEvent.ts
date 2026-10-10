/**
 * Agent SSE event types — mirrors `gglib_core::domain::agent::AgentEvent`.
 *
 * Each variant is tagged with a `type` field using snake_case, matching the
 * Rust `#[serde(tag = "type", rename_all = "snake_case")]` annotation.
 *
 * @module agentEvent
 */

import type { TurnUsageWire } from '../../utils/messages/turnMade';
import type { ToolCall } from '../generated/ToolCall';
import type { ToolResult } from '../generated/ToolResult';

// ---------------------------------------------------------------------------
// Embedded wire types (Rust ToolCall / ToolResult)
// ---------------------------------------------------------------------------

/** A tool invocation requested by the LLM: the Rust `ToolCall`, as generated from it. */
export type AgentToolCall = ToolCall;

/** The outcome of a tool execution: the Rust `ToolResult`, as generated from it. */
export type AgentToolResult = ToolResult;

// ---------------------------------------------------------------------------
// Discriminated union
// ---------------------------------------------------------------------------

/** An incremental text fragment from the model's response. */
export interface AgentTextDeltaEvent {
  type: 'text_delta';
  content: string;
}

/** The model has requested execution of a tool. */
export interface AgentToolCallStartEvent {
  type: 'tool_call_start';
  tool_call: AgentToolCall;
  /** Human-readable title-cased tool name (e.g. "Read File"). */
  display_name: string;
  /** Optional one-line argument summary (e.g. a file path). */
  args_summary?: string;
}

/** A tool execution has completed (success or failure). */
export interface AgentToolCallCompleteEvent {
  type: 'tool_call_complete';
  /** Raw tool name including any prefix (e.g. "builtin:read_file"). */
  tool_name: string;
  result: AgentToolResult;
  /** Time spent waiting for a concurrency slot, in milliseconds. */
  wait_ms: number;
  /** Wall-clock execution time (after acquiring the slot), in milliseconds. */
  execute_duration_ms: number;
  /** Human-readable title-cased tool name (e.g. "Read File"). */
  display_name: string;
  /** Pre-formatted duration string (e.g. "125ms", "1.8s"). */
  duration_display: string;
}

/** One full LLM → tool-execution cycle has completed. */
export interface AgentIterationCompleteEvent {
  type: 'iteration_complete';
  /** 1-based iteration index that just finished. */
  iteration: number;
  /** Number of tool calls executed during this iteration. */
  tool_calls: number;
}

/**
 * An incremental reasoning/thinking fragment (CoT tokens).
 *
 * Emitted by reasoning-capable models (e.g. DeepSeek R1, QwQ) that expose
 * their chain-of-thought.  These tokens are forwarded live but are excluded
 * from the conversation history sent back to the model.
 */
export interface AgentReasoningDeltaEvent {
  type: 'reasoning_delta';
  content: string;
}

/** The loop has concluded and produced a definitive answer. */
export interface AgentFinalAnswerEvent {
  type: 'final_answer';
  content: string;
}

/** A fatal error has terminated the loop. */
export interface AgentErrorEvent {
  type: 'error';
  message: string;
}

/**
 * A non-fatal condition the loop recovered from, or is recovering from.
 *
 * Unlike {@link AgentErrorEvent} this does **not** end the stream — more
 * events follow. Emitted for things like an upstream 503 being retried after
 * backoff, or the model requesting more parallel tool calls than the limit
 * allows.
 *
 * `suggested_action`, when present, is an actionable hint (often a CLI
 * command) that can be rendered verbatim.
 */
export interface AgentSystemWarningEvent {
  type: 'system_warning';
  message: string;
  suggested_action?: string | null;
}

/** Prompt pre-fill progress from the LLM backend. */
export interface AgentPromptProgressEvent {
  type: 'prompt_progress';
  processed: number;
  total: number;
  cached: number;
  time_ms: number;
}

/**
 * How one model turn was made, once its stream ended: the model (stamped by
 * the daemon's run), the upstream's token counts, and how long the turn took
 * and spent writing. Each field is absent when unknown.
 */
export interface AgentTurnUsageEvent extends TurnUsageWire {
  type: 'turn_usage';
}

/** Where a long-running tool has got to: the Rust `ToolStage`. */
export type ToolStage = 'queued' | 'loading' | 'sampling' | 'decoding' | 'finishing';

/**
 * How far a running tool has got, for a tool that takes minutes (an image
 * render). Logged with the run's other events, about once a second per
 * call; a count the tool did not report is absent.
 */
export interface AgentToolProgressEvent {
  type: 'tool_progress';
  tool_call_id: string;
  stage: ToolStage;
  /** Which pass is running, 1-based, when the work has several (one per image). */
  pass?: number;
  /** Steps done in this pass. */
  done?: number;
  /** Steps this pass takes. */
  total?: number;
  /** Place in line while queued, 1 being next. */
  position?: number;
}

/**
 * Nothing can happen until something else finishes: an image render holds
 * the GPU, or the model is loading. Sent again whenever what it reports
 * changes.
 */
export interface AgentWaitingEvent {
  type: 'waiting';
  reason: 'image_render' | 'model_load';
  /** The step the work in the way last reported; 0 before its first. */
  step: number;
  /** How many steps that work takes; 0 when unknown. */
  total: number;
  /** This wait's place in line, 1 being next; 0 when not in a line. */
  position: number;
}

/**
 * The latest preview of what a running tool is making: the payload of a
 * run's `preview` side event, which is sent beside the log and is not an
 * {@link AgentEvent}. Shown while its call runs, and never kept.
 */
export interface RunPreview {
  tool_call_id: string;
  frame: {
    /** The frame's media type; `image/png`. */
    mime: string;
    /** The step this frame shows. */
    step: number;
    /** How many steps the pass takes. */
    total: number;
    /** The frame's bytes, base64. */
    b64: string;
  };
}

/**
 * Union of all events emitted by the backend agentic loop over SSE.
 *
 * Consumers should handle all variants; unknown `type` values should be
 * ignored to remain forward-compatible with new variants added on the server.
 */
export type AgentEvent =
  | AgentTextDeltaEvent
  | AgentReasoningDeltaEvent
  | AgentToolCallStartEvent
  | AgentToolCallCompleteEvent
  | AgentToolProgressEvent
  | AgentWaitingEvent
  | AgentIterationCompleteEvent
  | AgentFinalAnswerEvent
  | AgentErrorEvent
  | AgentSystemWarningEvent
  | AgentPromptProgressEvent
  | AgentTurnUsageEvent;
