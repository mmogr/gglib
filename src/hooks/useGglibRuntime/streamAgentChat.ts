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
import { finalizeMessageTiming } from './agentMessageState';
import { dispatchAgentEvent, type DispatchDeps, type DispatchState } from './agentEventDispatch';
import { isAbortError } from '../../utils/errors';

// The dispatcher and its two types kept their public path when they moved:
// the tests import them from here.
export { dispatchAgentEvent, type DispatchDeps, type DispatchState } from './agentEventDispatch';

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
  /** Maps to `AgentConfig::max_parallel_tools` (default 25). */
  max_parallel_tools?: number;
  /** Maps to `AgentConfig::tool_timeout_ms` (default 30 000). */
  tool_timeout_ms?: number;
  /** Tool names classified as observations; `[]` disables classification. */
  observation_tools?: string[];
  /** Maps to `AgentConfig::max_observation_steps` (default 15). */
  max_observation_steps?: number;
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
   * The two reasoning controls, which `AgentChatRequest` takes at the **top
   * level** of the body rather than inside `config` — they are per-turn shape,
   * not agent-loop tuning. Spread verbatim, so an omitted field resolves from
   * the profile, per-model, global and floor layers as it always did.
   */
  reasoning?: { reasoning_effort?: string; reasoning_budget_tokens?: number };
  /**
   * When `false`, no tools are exposed to the model.
   * Forwarded to the backend as an empty `tool_filter`.
   * `null` / `undefined` → permissive (all tools available).
   */
  supportsToolCalls?: boolean | null;
  /**
   * Send this turn to the machine on the other end of the remote tunnel
   * (ADR 0012) instead of the server on `selectedServerPort`. The daemon
   * takes the tunnel's port and the stored key; `selectedServerPort` still
   * travels, and is not consulted.
   */
  remote?: boolean;
  /**
   * Called for each non-fatal `system_warning` the loop emits — an upstream
   * 503 being retried, a tool-call batch being trimmed. The stream continues
   * either way; this is purely so the user is told what is happening rather
   * than watching an idle cursor.
   */
  onSystemWarning?: (message: string, suggestedAction?: string | null) => void;
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
    reasoning,
    supportsToolCalls,
    onSystemWarning,
    remote = false,
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
      toolFilter = enabled.map((def) => registry.getBackendName(def.function.name));
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
        ...reasoning,
        ...(remote ? { remote: true } : {}),
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
  const dispatchDeps: DispatchDeps = {
    setMessages,
    timingTracker,
    makeNextMessage,
    cleanup,
    onSystemWarning,
  };
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
