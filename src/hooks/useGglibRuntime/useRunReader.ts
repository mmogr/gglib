/**
 * The open conversation's messages, and the run being read into them.
 *
 * Opening a conversation first asks whether a run is still going in it,
 * then loads its saved rows, then, if one was going, reads it from its
 * first event. In that order a run that ends at any point of the opening is
 * shown once: ended before the question, its reply is in the rows (a run
 * reads as ended only once its reply is saved); ended after, its stream
 * replays it, and at its end the messages become what the daemon saved:
 * the rows, their ids, and a reply marked unfinished when it was. Nothing
 * can be sent until the question and the rows have both come back.
 *
 * Leaving (another conversation, unmount) stops reading; it never cancels
 * the run. The run's id is kept in memory only.
 *
 * @module useRunReader
 */

import { useState, useRef, useEffect, useCallback } from 'react';
import type { GglibMessage } from '../../types/messages';
import { mkAssistantMessage } from '../../types/messages';
import type { ThreadConversation } from '../useChatPersistence/buildThreadMessages';
import { ReasoningTimingTracker } from './reasoningTiming';
import { performanceClock } from './clock';
import { drawRun, type RunOutcome } from './drawRun';
import { liveRunFor, loadSavedThread } from './savedRows';

/** What the reader reads from the caller's latest render. */
export interface RunReaderInputs {
  conversation?: ThreadConversation | null;
  onError?: (error: Error) => void;
  onSystemWarning?: (message: string, suggestedAction?: string | null) => void;
  onConversationChanged?: (conversationId: number) => void;
}

export function useRunReader(
  conversationId: number | undefined,
  inputs: RunReaderInputs,
) {
  const [messages, setMessages] = useState<GglibMessage[]>([]);
  const messagesRef = useRef(messages);
  useEffect(() => {
    messagesRef.current = messages;
  }, [messages]);

  // The latest inputs, for a reading that outlives the render that began it.
  const latest = useRef(inputs);
  useEffect(() => {
    latest.current = inputs;
  });

  const [isRunning, setIsRunning] = useState(false);
  const [isLoading, setIsLoading] = useState(false);
  const runningRef = useRef(false);
  /** An opening that has not yet learned whether a run is live. */
  const openingRef = useRef(false);
  /** The reading in progress. Aborting it stops reading, never the run. */
  const readerRef = useRef<AbortController | null>(null);
  /** The run read here, once the daemon has accepted it. */
  const runRef = useRef<string | null>(null);
  /** A conversation a send created and is already reading. */
  const adoptRef = useRef<number | null>(null);
  const [currentStreamingAssistantMessageId, setCurrentStreamingAssistantMessageId] =
    useState<string | null>(null);

  const timingTrackerRef = useRef(new ReasoningTimingTracker(performanceClock));
  const timingTracker = timingTrackerRef.current;
  useEffect(() => {
    timingTracker.clearAll();
  }, [conversationId, timingTracker]);

  const setRunning = useCallback((running: boolean) => {
    runningRef.current = running;
    setIsRunning(running);
    if (!running) {
      runRef.current = null;
      setCurrentStreamingAssistantMessageId(null);
    }
  }, []);

  /** Start a reading, ending any other. */
  const beginReading = useCallback((): AbortSignal => {
    readerRef.current?.abort();
    const controller = new AbortController();
    readerRef.current = controller;
    return controller.signal;
  }, []);

  /** Leave: stop reading. A run being read carries on at the daemon. */
  const stopReading = useCallback(() => {
    openingRef.current = false;
    readerRef.current?.abort();
    readerRef.current = null;
    setRunning(false);
  }, [setRunning]);

  /** The reading `signal` belongs to came to its own end. */
  const endReading = useCallback((signal: AbortSignal) => {
    if (readerRef.current?.signal !== signal) return;
    readerRef.current = null;
    setRunning(false);
  }, [setRunning]);

  /** Show what is saved in `cid`, unless the reading was left. */
  const showSaved = useCallback(async (cid: number, signal: AbortSignal) => {
    const thread = await loadSavedThread(cid, latest.current.conversation ?? null);
    if (signal.aborted) return;
    messagesRef.current = thread;
    setMessages(thread);
  }, []);

  /** Read run `runId` from its first event, then show what the daemon saved. */
  const follow = useCallback(async (cid: number, runId: string, signal: AbortSignal) => {
    runRef.current = runId;
    runningRef.current = true;
    setIsRunning(true);
    const unlessLeft = <T>(set: (value: T) => void) => (value: T) => {
      if (!signal.aborted) set(value);
    };
    let outcome: RunOutcome;
    try {
      outcome = await drawRun({
        runId,
        turnId: crypto.randomUUID(),
        conversationId: cid,
        signal,
        setMessages: unlessLeft(setMessages),
        mkAssistantMessage,
        timingTracker,
        setCurrentStreamingAssistantMessageId: unlessLeft(setCurrentStreamingAssistantMessageId),
        onSystemWarning: (message, action) => latest.current.onSystemWarning?.(message, action),
      });
    } catch (error) {
      if (signal.aborted) return;
      outcome = { info: null, error: error as Error };
    }
    if (signal.aborted) return;
    try {
      await showSaved(cid, signal);
    } catch (error) {
      outcome.error ??= error as Error;
    }
    if (signal.aborted) return;
    endReading(signal);
    latest.current.onConversationChanged?.(cid);
    if (outcome.error) latest.current.onError?.(outcome.error);
  }, [endReading, showSaved, timingTracker]);

  /** Learn whether a run is live in `cid`, show its rows, then read the run. */
  const open = useCallback(async (cid: number, signal: AbortSignal) => {
    openingRef.current = true;
    setIsLoading(true);
    try {
      const live = await liveRunFor(cid);
      if (signal.aborted) return;
      await showSaved(cid, signal);
      if (signal.aborted) return;
      openingRef.current = false;
      setIsLoading(false);
      if (live) await follow(cid, live.id, signal);
    } catch (error) {
      if (!signal.aborted) latest.current.onError?.(error as Error);
    } finally {
      if (!signal.aborted) {
        openingRef.current = false;
        setIsLoading(false);
      }
    }
  }, [follow, showSaved]);

  const systemPrompt = inputs.conversation?.system_prompt;
  useEffect(() => {
    if (conversationId === undefined) {
      openingRef.current = false;
      setMessages([]);
      setIsLoading(false);
      return undefined;
    }
    const adopted = adoptRef.current === conversationId;
    adoptRef.current = null;
    if (!adopted) void open(conversationId, beginReading());
    return stopReading;
  }, [conversationId, systemPrompt, open, beginReading, stopReading]);

  /**
   * Claim the conversation for a send: null while a run is live in it, or
   * while opening it has not yet learned whether one is.
   */
  const beginSend = useCallback((): AbortSignal | null => {
    if (runningRef.current || openingRef.current) return null;
    runningRef.current = true;
    setIsRunning(true);
    return beginReading();
  }, [beginReading]);

  return {
    messages,
    setMessages,
    messagesRef,
    isRunning,
    isLoading,
    timingTracker,
    currentStreamingAssistantMessageId,
    /** The run being read here, if the daemon has accepted it. */
    liveRunId: () => runRef.current,
    /** A send created `cid`: its opening must not interrupt the send's reading. */
    adopt: (cid: number) => {
      adoptRef.current = cid;
    },
    beginSend,
    endReading,
    showSaved,
    follow,
  };
}
