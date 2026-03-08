/**
 * Backend-driven agentic loop consumer.
 *
 * POSTs the conversation history to `POST /api/agent/chat` and processes the
 * Server-Sent Event stream that the backend emits for each observable step of
 * the loop (text deltas, tool calls, iterations, final answer).
 *
 * # Message-per-iteration model
 *
 * One `GglibMessage` (role = "assistant") is created per backend iteration,
 * preserving the UI pattern established by the old frontend loop:
 *
 * - Tool-calling iterations: text_delta* → tool_call_start* →
 *   tool_call_complete* → iteration_complete  → new assistant message
 * - Final-answer iteration:  text_delta* → final_answer               (done)
 *
 * @module streamAgentChat
 */

import React from 'react';

import { appLogger } from '../../services/platform';
import { getAuthenticatedFetchConfig } from '../../services/transport/api/client';
import { getToolRegistry } from '../../services/tools';
import type { GglibMessage, GglibMessageCustom } from '../../types/messages';
import type { AgentEvent } from '../../types/events/agentEvent';
import type { ReasoningTimingTracker } from './reasoningTiming';
import { convertToWireMessages } from './wireMessages';
import { readAgentSSE } from './agentSseReader';
import {
  applyTextDelta,
  applyReasoningDelta,
  addToolCallPart,
  applyToolResult,
  finalizeMessageTiming,
  setFullText,
} from './agentMessageState';
import { isAbortError } from '../../utils/errors';

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/**
 * Partial `AgentConfig` forwarded to the backend.
 *
 * Only includes fields exposed by `AgentRequestConfig` in `gglib-axum`.
 * Internal tuning parameters (`max_stagnation_steps`, `context_budget_chars`,
 * `max_repeated_batch_steps`, `prune_*`) are intentionally absent from the
 * backend DTO to prevent resource exhaustion by untrusted callers; omit them
 * here to avoid silently sending values the server will discard.
 *
 * All fields are optional; omitted fields use the backend's
 * `AgentConfig::default()` values.
 */
export interface PartialAgentConfig {
  /** Maps to `AgentConfig::max_iterations` (default 25). */
  max_iterations?: number;
  /** Maps to `AgentConfig::max_parallel_tools` (default 5). */
  max_parallel_tools?: number;
  /** Maps to `AgentConfig::tool_timeout_ms` (default 30 000). */
  tool_timeout_ms?: number;
}

export interface StreamAgentChatOptions {
  turnId: string;
  getMessages: () => GglibMessage[];
  setMessages: React.Dispatch<React.SetStateAction<GglibMessage[]>>;
  selectedServerPort: number;
  abortSignal?: AbortSignal;
  conversationId?: number;
  mkAssistantMessage: (custom?: GglibMessageCustom) => GglibMessage;
  timingTracker?: ReasoningTimingTracker;
  setCurrentStreamingAssistantMessageId?: (id: string | null) => void;
  /** Optional partial `AgentConfig` overrides; omitted fields use backend defaults. */
  config?: PartialAgentConfig;
  /**
   * When `false`, no tools are exposed to the model.
   * Forwarded to the backend as an empty `tool_filter`.
   * `null` / `undefined` → permissive (all tools available).
   */
  supportsToolCalls?: boolean | null;
}

// ---------------------------------------------------------------------------
// Main export
// ---------------------------------------------------------------------------

/**
 * Stream an agentic conversation against the backend `/api/agent/chat`
 * endpoint and update React state with each incoming event.
 *
 * The function resolves when the loop ends with a `final_answer` event or
 * user abort.  It **throws** when the backend emits an `error` event
 * (fatal loop failure) or when the HTTP request itself fails, so callers
 * can surface the failure through their error-handling path (e.g. `onError`).
 */
export async function streamAgentChat(options: StreamAgentChatOptions): Promise<void> {
  const {
    turnId,
    getMessages,
    setMessages,
    selectedServerPort,
    abortSignal,
    conversationId,
    mkAssistantMessage,
    timingTracker,
    setCurrentStreamingAssistantMessageId,
    config,
    supportsToolCalls,
  } = options;

  // Build agent config: use null to let the backend apply defaults unless
  // the caller has overridden at least one field.  Strip `undefined` values
  // so `{ max_iterations: undefined }` does not produce a spurious key.
  const agentConfig: Record<string, unknown> | null = (() => {
    if (!config) return null;
    const defined = Object.fromEntries(
      Object.entries(config).filter(([, v]) => v !== undefined),
    );
    return Object.keys(defined).length > 0 ? defined : null;
  })();

  // Tool filter: when the model supports tool calls, forward the explicit list
  // of enabled tools in backend qualified-name format ("serverId:originalName").
  // An empty array strips all tools (model known not to support tool-calling).
  // null means "no filter" — never sent when we have registry entries.
  let toolFilter: string[] | null;
  if (supportsToolCalls === false) {
    toolFilter = [];
  } else {
    const registry = getToolRegistry();
    const enabled = registry.getEnabledDefinitions();
    if (enabled.length === 0) {
      toolFilter = null;
    } else {
      toolFilter = enabled.map((def) => {
        const sanitized = def.function.name;
        const serverId = registry.getServerId(sanitized);
        const original = registry.getOriginalName(sanitized);
        if (serverId !== undefined && original !== undefined) {
          return `${serverId}:${original}`;
        }
        // Fallback for tools registered via register() without a name mapping
        return sanitized;
      });
    }
  }

  // ── Authenticate and resolve backend base URL ─────────────────────────────
  const { baseUrl, headers: authHeaders } = await getAuthenticatedFetchConfig();

  // ── Convert UI messages to backend wire format ────────────────────────────
  const wireMessages = convertToWireMessages(getMessages());

  appLogger.debug('hook.runtime', 'streamAgentChat: starting', {
    port: selectedServerPort,
    messages: wireMessages.length,
  });

  // ── POST the request ──────────────────────────────────────────────────────
  let response: Response;
  try {
    response = await fetch(`${baseUrl}/api/agent/chat`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(authHeaders as Record<string, string>),
      },
      body: JSON.stringify({
        port: selectedServerPort,
        messages: wireMessages,
        config: agentConfig,
        tool_filter: toolFilter,
      }),
      signal: abortSignal,
    });
  } catch (err) {
    if (isAbortError(err)) return;
    throw err;
  }

  if (!response.ok) {
    const text = await response.text().catch(() => '');
    throw new Error(`Agent chat request failed: ${response.status} ${text}`);
  }

  // ── Create the first assistant message ───────────────────────────────────
  // Takes an explicit `iter` parameter so every caller site is unambiguous
  // about which iteration number it is requesting (avoids hidden mutable state).
  const makeNextMessage = (iter: number): string => {
    const msg = mkAssistantMessage({ turnId, iteration: iter, conversationId });
    if (!msg.id) throw new Error('mkAssistantMessage must return a message with an id');
    setMessages(prev => [...prev, msg]);
    setCurrentStreamingAssistantMessageId?.(msg.id);
    return msg.id;
  };

  const state: DispatchState = { currentId: makeNextMessage(1) };

  // Finalize the current in-progress message and clear the streaming indicator.
  const cleanup = (): void => {
    finalizeMessageTiming(setMessages, state.currentId);
    setCurrentStreamingAssistantMessageId?.(null);
  };

  // -- Process the SSE stream --------------------------------------------------
  const dispatchDeps: DispatchDeps = { setMessages, timingTracker, makeNextMessage, cleanup };
  try {
    for await (const payload of readAgentSSE(response, abortSignal)) {
      let event: AgentEvent;
      try {
        event = JSON.parse(payload) as AgentEvent;
      } catch {
        appLogger.warn('hook.runtime', 'streamAgentChat: ignoring unparseable SSE payload', { payload });
        continue;
      }
      if (dispatchAgentEvent(event, state, dispatchDeps)) return;
    }
  } catch (err) {
    if (isAbortError(err)) {
      // User cancelled — finalize the current message cleanly.
      cleanup();
      return;
    }
    // Non-abort error (network failure, protocol violation, etc.) — finalize
    // the in-progress message so it is never left permanently "in-flight".
    cleanup();
    appLogger.error('hook.runtime', 'streamAgentChat: stream error', { err });
    throw err;
  }

  // Stream ended without a final_answer or error event — the server shut down
  // or the connection was dropped mid-stream.  Log a warning so the gap is
  // visible in diagnostics, then finalize whatever partial message exists.
  appLogger.warn('hook.runtime', 'streamAgentChat: SSE stream ended without final_answer or error event');
  cleanup();
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

export interface DispatchState {
  /** ID of the current in-progress assistant message. Mutated on iteration_complete. */
  currentId: string;
}

export interface DispatchDeps {
  setMessages: React.Dispatch<React.SetStateAction<GglibMessage[]>>;
  timingTracker: ReasoningTimingTracker | undefined;
  makeNextMessage: (iter: number) => string;
  cleanup: () => void;
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
  const { setMessages, timingTracker, makeNextMessage, cleanup } = deps;

  switch (event.type) {
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
      addToolCallPart(setMessages, state.currentId, event.tool_call.id, event.tool_call.name, event.tool_call.arguments);
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
