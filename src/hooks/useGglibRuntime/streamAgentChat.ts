/**
 * Backend-driven agentic loop consumer.
 *
 * POSTs the conversation history to `POST /api/agent/chat` and processes the
 * Server-Sent Event stream that the backend emits for each observable step of
 * the loop (text deltas, tool calls, iterations, final answer).
 *
 * This replaces the client-side `runAgenticLoop` / `streamModelResponse` /
 * `executeToolBatch` stack — all loop orchestration now lives in Rust.
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
} from './agentMessageState';

// ---------------------------------------------------------------------------
// Utilities
// ---------------------------------------------------------------------------

/**
 * Type-safe AbortError predicate.
 *
 * `fetch()` and `ReadableStream.read()` both throw a `DOMException` with
 * `name === 'AbortError'` when an `AbortSignal` fires.  `DOMException`
 * implements `Error` in all modern environments, so an `instanceof Error`
 * guard works — but the cast pattern `(err as Error).name` does not guard
 * the type at all and silently passes through non-Error rejections.  This
 * predicate uses `instanceof Error` as the structural guard and then checks
 * the discriminating `name` property.
 */
function isAbortError(err: unknown): err is DOMException {
  return err instanceof Error && err.name === 'AbortError';
}

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/**
 * Partial `AgentConfig` forwarded to the backend.  All fields are optional;
 * omitted fields use the backend's `AgentConfig::default()` values.
 */
export interface PartialAgentConfig {
  /** Maps to `AgentConfig::max_iterations` (default 25). */
  max_iterations?: number;
  /** Maps to `AgentConfig::max_stagnation_steps` (default 5). */
  max_stagnation_steps?: number;
  /** Maps to `AgentConfig::tool_timeout_ms` (default 30 000). */
  tool_timeout_ms?: number;
  /** Maps to `AgentConfig::context_budget_chars` (default 180 000). */
  context_budget_chars?: number;
  /** Maps to `AgentConfig::max_protocol_strikes` (default 2). */
  max_protocol_strikes?: number;
  /** Maps to `AgentConfig::max_parallel_tools` (default 5). */
  max_parallel_tools?: number;
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
  // the caller has overridden at least one field.
  const agentConfig: Record<string, unknown> | null =
    config && Object.keys(config).length > 0 ? { ...config } : null;

  // Tool filter: empty array strips all tools when model is known to not
  // support tool-calling (explicit false only — null/undefined is permissive).
  const toolFilter: string[] | null = supportsToolCalls === false ? [] : null;

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
    setMessages(prev => [...prev, msg]);
    setCurrentStreamingAssistantMessageId?.(msg.id!);
    return msg.id!;
  };

  let currentId = makeNextMessage(1);

  // ── Process the SSE stream ────────────────────────────────────────────────
  try {
    for await (const payload of readAgentSSE(response, abortSignal)) {
      let event: AgentEvent;
      try {
        event = JSON.parse(payload) as AgentEvent;
      } catch {
        appLogger.warn('hook.runtime', 'streamAgentChat: ignoring unparseable SSE payload', { payload });
        continue;
      }

      switch (event.type) {
        case 'reasoning_delta': {
          applyReasoningDelta(setMessages, currentId, event.content);
          break;
        }

        case 'text_delta': {
          if (timingTracker) timingTracker.onBoundary(currentId);
          applyTextDelta(setMessages, currentId, event.content);
          break;
        }

        case 'tool_call_start': {
          if (timingTracker) timingTracker.onBoundary(currentId);
          addToolCallPart(
            setMessages,
            currentId,
            event.tool_call.id,
            event.tool_call.name,
            event.tool_call.arguments,
          );
          appLogger.debug('hook.runtime', 'streamAgentChat: tool call started', {
            tool: event.tool_call.name,
          });
          break;
        }

        case 'tool_call_complete': {
          applyToolResult(setMessages, currentId, event);
          appLogger.debug('hook.runtime', 'streamAgentChat: tool call complete', {
            id: event.result.tool_call_id,
            success: event.result.success,
            waitMs: event.wait_ms,
            durationMs: event.execute_duration_ms,
          });
          break;
        }

        case 'iteration_complete': {
          // Finalize the current message and open a fresh one for the next
          // iteration (which will start with its own text_delta stream).
          if (timingTracker) timingTracker.onEndOfMessage(currentId);
          finalizeMessageTiming(setMessages, currentId);
          setCurrentStreamingAssistantMessageId?.(null);

          appLogger.debug('hook.runtime', 'streamAgentChat: iteration complete', {
            iteration: event.iteration,
            toolCalls: event.tool_calls,
          });

          currentId = makeNextMessage(event.iteration + 1);
          break;
        }

        case 'final_answer': {
          // The stream has ended normally.  The accumulated text_deltas have
          // already built the message content; finalize timing and stop.
          if (timingTracker) timingTracker.onEndOfMessage(currentId);
          finalizeMessageTiming(setMessages, currentId);
          setCurrentStreamingAssistantMessageId?.(null);

          appLogger.info('hook.runtime', 'streamAgentChat: final answer', {
            contentLength: event.content.length,
          });
          return;
        }

        case 'error': {
          // Fatal backend error: clean up the in-progress message then throw
          // so the caller's catch block can invoke its onError callback.
          if (timingTracker) timingTracker.onEndOfMessage(currentId);
          finalizeMessageTiming(setMessages, currentId);
          setCurrentStreamingAssistantMessageId?.(null);

          appLogger.warn('hook.runtime', 'streamAgentChat: agent error event', {
            message: event.message,
          });
          throw new Error(`Agent loop error: ${event.message}`);
        }

        default: {
          // Forward-compatibility: ignore unknown event types.
          appLogger.debug('hook.runtime', 'streamAgentChat: unknown event type, skipping', {
            type: (event as { type: string }).type,
          });
        }
      }
    }
  } catch (err) {
    if (isAbortError(err)) {
      // User cancelled — finalize the current message cleanly.
      finalizeMessageTiming(setMessages, currentId);
      setCurrentStreamingAssistantMessageId?.(null);
      return;
    }
    appLogger.error('hook.runtime', 'streamAgentChat: stream error', { err });
    throw err;
  }

  // Stream ended without a final_answer or error event — treat as complete.
  finalizeMessageTiming(setMessages, currentId);
  setCurrentStreamingAssistantMessageId?.(null);
}
