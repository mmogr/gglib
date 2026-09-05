/**
 * One SSE `AgentEvent` → React message state.
 *
 * Split from `streamAgentChat.ts`, unchanged, when the remote flag arrived and
 * that file was at its budget. `streamAgentChat` owns the request and the
 * stream; this owns what each event does to the messages.
 *
 * @module agentEventDispatch
 */

import React from 'react';

import { appLogger } from '../../services/platform';
import type { GglibMessage } from '../../types/messages';
import type { AgentEvent } from '../../types/events/agentEvent';
import type { ReasoningTimingTracker } from './reasoningTiming';
import {
  applyTextDelta,
  applyReasoningDelta,
  addToolCallPart,
  applyToolResult,
  setFullText,
} from './agentMessageState';

export interface DispatchState {
  /** ID of the current in-progress assistant message. Mutated on iteration_complete. */
  currentId: string;
}

export interface DispatchDeps {
  setMessages: React.Dispatch<React.SetStateAction<GglibMessage[]>>;
  timingTracker: ReasoningTimingTracker | undefined;
  makeNextMessage: (iter: number) => string;
  cleanup: () => void;
  /** Surfaces non-fatal `system_warning` events; see {@link StreamAgentChatOptions}. */
  onSystemWarning?: (message: string, suggestedAction?: string | null) => void;
}

/**
 * Handle one SSE {@link AgentEvent}, mutating React message state in-place.
 *
 * Mutates `state.currentId` on `iteration_complete` (new message for next turn).
 *
 * @returns `true` when the stream is complete (`final_answer`), `false` to
 *          continue consuming.  Throws on `error` events (fatal backend failure).
 *
 * Exported for unit testing — callers outside this module should use
 * {@link streamAgentChat} instead.
 */
export function dispatchAgentEvent(event: AgentEvent, state: DispatchState, deps: DispatchDeps): boolean {
  const { setMessages, timingTracker, makeNextMessage, cleanup, onSystemWarning } = deps;

  switch (event.type) {
    case 'system_warning': {
      // Non-fatal by definition: the loop is still running and more events
      // follow, so this must not cleanup(), throw, or end the stream.
      //
      // Without this case the event fell through to the forward-compatibility
      // default below and was discarded, which meant retry notices — and the
      // parallel-tool-limit warning that predates them — were invisible in the
      // GUI even though the CLI renderer had always shown them.
      appLogger.warn('hook.runtime', 'streamAgentChat: system warning', {
        message: event.message,
        suggestedAction: event.suggested_action ?? undefined,
      });
      onSystemWarning?.(event.message, event.suggested_action);
      return false;
    }

    case 'reasoning_delta': {
      if (typeof event.content !== 'string') {
        appLogger.warn('hook.runtime', 'streamAgentChat: reasoning_delta missing content string', { event });
        return false;
      }
      if (timingTracker) timingTracker.onReasoning(state.currentId);
      applyReasoningDelta(setMessages, state.currentId, event.content);
      return false;
    }

    case 'text_delta': {
      if (typeof event.content !== 'string') {
        appLogger.warn('hook.runtime', 'streamAgentChat: text_delta missing content string', { event });
        return false;
      }
      if (timingTracker) timingTracker.onBoundary(state.currentId);
      applyTextDelta(setMessages, state.currentId, event.content);
      return false;
    }

    case 'tool_call_start': {
      if (!event.tool_call || typeof event.tool_call.id !== 'string' || typeof event.tool_call.name !== 'string') {
        appLogger.warn('hook.runtime', 'streamAgentChat: tool_call_start malformed', { event });
        return false;
      }
      if (timingTracker) timingTracker.onBoundary(state.currentId);
      addToolCallPart(setMessages, state.currentId, event.tool_call.id, event.tool_call.name, event.tool_call.arguments, event.display_name);
      appLogger.debug('hook.runtime', 'streamAgentChat: tool call started', { tool: event.tool_call.name });
      return false;
    }

    case 'tool_call_complete': {
      if (!event.result || typeof event.result.tool_call_id !== 'string') {
        appLogger.warn('hook.runtime', 'streamAgentChat: tool_call_complete malformed', { event });
        return false;
      }
      applyToolResult(setMessages, state.currentId, event);
      appLogger.debug('hook.runtime', 'streamAgentChat: tool call complete', {
        id: event.result.tool_call_id,
        success: event.result.success,
        waitMs: event.wait_ms,
        durationMs: event.execute_duration_ms,
      });
      return false;
    }

    case 'iteration_complete': {
      // Finalize the current message and open a fresh one for the next iteration.
      if (timingTracker) timingTracker.onEndOfMessage(state.currentId);
      cleanup();
      appLogger.debug('hook.runtime', 'streamAgentChat: iteration complete', {
        iteration: event.iteration,
        toolCalls: event.tool_calls,
      });
      state.currentId = makeNextMessage(event.iteration + 1);
      return false;
    }

    case 'final_answer': {
      if (typeof event.content !== 'string') {
        appLogger.warn('hook.runtime', 'streamAgentChat: final_answer missing content string', { event });
      } else {
        // Defensive replacement: the complete answer text is redundant with
        // the preceding text_delta events, but if any delta was lost in
        // transit the message would be left with partial content.  Replacing
        // the full text part here guarantees the final message is always
        // complete — even after a lossy transport.
        setFullText(setMessages, state.currentId, event.content);
      }
      if (timingTracker) timingTracker.onEndOfMessage(state.currentId);
      cleanup();
      appLogger.info('hook.runtime', 'streamAgentChat: final answer', {
        contentLength: typeof event.content === 'string' ? event.content.length : null,
      });
      return true;
    }

    case 'error': {
      if (timingTracker) timingTracker.onEndOfMessage(state.currentId);
      cleanup();
      appLogger.warn('hook.runtime', 'streamAgentChat: agent error event', { message: event.message });
      throw new Error(`Agent loop error: ${String(event.message ?? 'unknown agent error')}`);
    }

    default: {
      // Forward-compatibility: ignore unknown event types.
      appLogger.debug('hook.runtime', 'streamAgentChat: unknown event type, skipping', {
        type: (event as { type: string }).type,
      });
      return false;
    }
  }
}
