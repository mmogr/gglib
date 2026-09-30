/**
 * Draw a run's reply from its events, from the first.
 *
 * One assistant message per loop iteration, as the agent route always drew
 * it: text and reasoning deltas and tool calls go to the current message,
 * `iteration_complete` opens the next. What is drawn is provisional: once the
 * run ends the page shows the rows the daemon saved instead.
 *
 * @module drawRun
 */

import React from 'react';

import type { ChatSource } from '../../services/transport';
import type { GglibMessage, GglibMessageCustom } from '../../types/messages';
import type { AgentEvent } from '../../types/events/agentEvent';
import type { RunInfo } from '../../types/generated/RunInfo';
import type { ReasoningTimingTracker } from './reasoningTiming';
import { finalizeMessageTiming } from './agentMessageState';
import { dispatchAgentEvent, type DispatchDeps, type DispatchState } from './agentEventDispatch';
import { runsOf } from './chatSource';

export interface DrawRunOptions {
  runId: string;
  turnId: string;
  conversationId: number;
  /** The machine whose run it is; this one when absent. */
  source?: ChatSource;
  /** Stops the reading; the run carries on. */
  signal: AbortSignal;
  setMessages: React.Dispatch<React.SetStateAction<GglibMessage[]>>;
  mkAssistantMessage: (custom?: GglibMessageCustom) => GglibMessage;
  timingTracker?: ReasoningTimingTracker;
  setCurrentStreamingAssistantMessageId?: (id: string | null) => void;
  onSystemWarning?: (message: string, suggestedAction?: string | null) => void;
}

/** How a drawn run ended, when its end was read. */
export interface RunOutcome {
  /** The run's final state; `null` when the stream closed before it came. */
  info: RunInfo | null;
  /** Why the reply failed, as its `error` event or the run's end says. */
  error: Error | null;
}

/** The failure a run's end reports when no `error` event said it first. */
function endFailure(info: RunInfo): Error | null {
  if (info.status !== 'failed') return null;
  return new Error(info.error?.message ?? 'The reply failed.');
}

function parseEvent(data: string): AgentEvent | null {
  try {
    return JSON.parse(data) as AgentEvent;
  } catch {
    return null;
  }
}

/**
 * Read run `runId` from its first event and draw it, until its end.
 *
 * Rejects only when the reading does (an unknown run, a dropped connection,
 * an abort); a reply that failed resolves with its error.
 */
export async function drawRun(options: DrawRunOptions): Promise<RunOutcome> {
  const {
    runId,
    turnId,
    conversationId,
    source = 'this',
    signal,
    setMessages,
    mkAssistantMessage,
    timingTracker,
    setCurrentStreamingAssistantMessageId,
    onSystemWarning,
  } = options;

  const makeNextMessage = (iteration: number): string => {
    const msg = mkAssistantMessage({ turnId, iteration, conversationId });
    setMessages((prev) => [...prev, msg]);
    setCurrentStreamingAssistantMessageId?.(msg.id!);
    return msg.id!;
  };
  const state: DispatchState = { currentId: makeNextMessage(1) };
  const cleanup = (): void => {
    finalizeMessageTiming(setMessages, state.currentId);
    setCurrentStreamingAssistantMessageId?.(null);
  };
  const deps: DispatchDeps = { setMessages, timingTracker, makeNextMessage, cleanup, onSystemWarning };

  let error: Error | null = null;
  // After `final_answer` or `error` the reply is settled; nothing after is drawn.
  let settled = false;
  for await (const item of runsOf(source).readRunEvents(runId, 0, signal)) {
    if (item.type === 'end') {
      if (!settled) cleanup();
      return { info: item.info, error: error ?? endFailure(item.info) };
    }
    const event = settled ? null : parseEvent(item.data);
    if (!event) continue;
    try {
      settled = dispatchAgentEvent(event, state, deps);
    } catch (e) {
      error = e instanceof Error ? e : new Error(String(e));
      settled = true;
    }
  }
  if (!settled) cleanup();
  return { info: null, error };
}
