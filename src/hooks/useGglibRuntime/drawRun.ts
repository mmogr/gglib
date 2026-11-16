/**
 * Draw a run's reply from its events, from the first.
 *
 * A stream that ends before the run's end is read again from the last event
 * drawn when the caller's `again` says so, into the same message: nothing
 * is drawn twice and nothing between the two readings is lost.
 *
 * One assistant message per loop iteration, as the agent route always drew
 * it: text and reasoning deltas and tool calls go to the current message,
 * `iteration_complete` opens the next. What is drawn is provisional: once the
 * run ends the page shows the rows the daemon saved instead.
 *
 * A `preview` item is no event of the reply: its frame goes to the caller's
 * `setPreviews`, beside the messages, until its tool call's result is drawn.
 * None is shown for a call whose result came, and none outlives the reading.
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
import { NO_PREVIEWS, parsePreview, withPreview, withoutPreview, type SetRunPreviews } from './runPreviews';

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
  /** Changes the preview frames held beside the messages; none are shown without it. */
  setPreviews?: SetRunPreviews;
  /**
   * Asked when the stream ends before the run does, with its error (`null`:
   * it closed) and whether it read a new event: true reads it again from
   * there. Without it, the reading ends there.
   */
  again?: (error: Error | null, progressed: boolean) => Promise<boolean>;
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
 * Rejects only when the reading does (an unknown run, a dropped connection
 * not read again, an abort); a reply that failed resolves with its error,
 * and a stream that closed early and was not read again with `info: null`.
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
    setPreviews,
    again,
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
  /** The tool calls whose result was drawn: a frame of one is late, and dropped. */
  const over = new Set<string>();
  /** The last event read: a stream read again starts after it. */
  let cursor = 0;
  try {
    for (;;) {
      const from = cursor;
      let broke: Error | null = null;
      try {
        for await (const item of runsOf(source).readRunEvents(runId, cursor, signal)) {
          if (item.type === 'end') {
            if (!settled) cleanup();
            return { info: item.info, error: error ?? endFailure(item.info) };
          }
          if (item.type === 'preview') {
            // Shown beside the reply, never part of it.
            const preview = settled || over.has(item.toolCallId) ? null : parsePreview(item.data);
            if (preview) setPreviews?.((held) => withPreview(held, preview));
            continue;
          }
          // Read, drawn or not: a stream read again does not bring it twice.
          cursor = Math.max(cursor, item.seq);
          const event = settled ? null : parseEvent(item.data);
          if (!event) continue;
          try {
            settled = dispatchAgentEvent(event, state, deps);
          } catch (e) {
            error = e instanceof Error ? e : new Error(String(e));
            settled = true;
          }
          const ended = event.type === 'tool_call_complete' ? event.result?.tool_call_id : undefined;
          if (typeof ended === 'string') {
            over.add(ended);
            setPreviews?.((held) => withoutPreview(held, ended));
          }
        }
      } catch (e) {
        if (signal.aborted || !again) throw e;
        broke = e instanceof Error ? e : new Error(String(e));
      }
      // The stream ended before the run did.
      if (!(await again?.(broke, cursor > from))) {
        if (broke) throw broke;
        break;
      }
    }
    if (!settled) cleanup();
    return { info: null, error };
  } finally {
    // However the reading ends, no frame outlives it.
    setPreviews?.(() => NO_PREVIEWS);
  }
}
