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
 * Nothing is ever sent from a conversation that is not loaded. A question
 * that fails still loads the rows, and the run is taken as not live; the
 * next send asks again first, and does not go while a run is live or while
 * the answer is still unknown. Rows that fail to load leave the thread
 * empty and sending off until the conversation is opened again.
 *
 * Leaving (another conversation, unmount) stops reading; it never cancels
 * the run. The run's id is kept in memory only. A far chat is read the same
 * way from the far machine, and nothing of it is kept; each reading of it is
 * handed up (`onFarOpened`), for what the far list does not say of a chat.
 *
 * The preview frames of the run being read (`previews`) are held here beside
 * the messages, never in them, and only while that run is read.
 *
 * @module useRunReader
 */

import { useState, useRef, useEffect, useCallback } from 'react';
import type { ChatSource } from '../../services/transport';
import type { GglibMessage } from '../../types/messages';
import { mkAssistantMessage } from '../../types/messages';
import type { HubChatOpen } from '../../types/generated/HubChatOpen';
import type { RunInfo } from '../../types/generated/RunInfo';
import type { ThreadConversation } from '../useChatPersistence/buildThreadMessages';
import { ReasoningTimingTracker } from './reasoningTiming';
import { performanceClock } from './clock';
import { drawRun, type RunOutcome } from './drawRun';
import { liveRunFor, loadSavedThread, type SavedView } from './savedRows';
import { NO_PREVIEWS, type RunPreviews } from './runPreviews';

const NOT_LOADED =
  'This conversation could not be loaded, so nothing can be sent from it. Open it again to retry.';
const UNKNOWN_LIVE =
  'Nothing was sent: whether a reply is still running in this conversation could not be checked.';
const STILL_RUNNING = 'Nothing was sent: a reply is still running in this conversation.';
const NOTHING_BESIDE = { answerable: false, points: [] };

/** What the reader reads from the caller's latest render. */
export interface RunReaderInputs {
  conversation?: ThreadConversation | null;
  /** Whose chat `conversationId` is; this machine's when absent. */
  source?: ChatSource;
  onError?: (error: Error) => void;
  onSystemWarning?: (message: string, suggestedAction?: string | null) => void;
  onConversationChanged?: (conversationId: number) => void;
  /** A far chat as the far machine just answered it, each time it is read: at opening, after a run or a refused send. */
  onFarOpened?: (open: HubChatOpen) => void;
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

  /** What the daemon says beside the open conversation's messages. */
  const [beside, setBeside] = useState<Omit<SavedView, 'messages'>>(NOTHING_BESIDE);
  const [isRunning, setIsRunning] = useState(false);
  const [isLoading, setIsLoading] = useState(false);
  const runningRef = useRef(false);
  /** An opening that has not yet learned whether a run is live. */
  const openingRef = useRef(false);
  /** The open conversation's rows could not be loaded: nothing may be sent. */
  const unloadedRef = useRef(false);
  /** Whether a run is live in it is unknown: a send must ask first. */
  const unaskedRef = useRef(false);
  /** The reading in progress. Aborting it stops reading, never the run. */
  const readerRef = useRef<AbortController | null>(null);
  /** The run read here, once the daemon has accepted it. */
  const runRef = useRef<string | null>(null);
  /** A conversation a send created and is already reading. */
  const adoptRef = useRef<number | null>(null);
  /** The run last read to its end here, as it ended: never one that ended unread. None once the conversation is left. */
  const [endedRun, setEndedRun] = useState<RunInfo | null>(null);
  const [currentStreamingAssistantMessageId, setCurrentStreamingAssistantMessageId] =
    useState<string | null>(null);

  /** The frames a tool of the run being read is making; none between readings. */
  const [previews, setPreviews] = useState<RunPreviews>(NO_PREVIEWS);

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
    unloadedRef.current = false;
    unaskedRef.current = false;
    readerRef.current?.abort();
    readerRef.current = null;
    // A reading that is left clears nothing itself: what it held goes here.
    setPreviews(NO_PREVIEWS);
    setRunning(false);
    setEndedRun(null);
  }, [setRunning]);

  /** The reading `signal` belongs to came to its own end. */
  const endReading = useCallback((signal: AbortSignal) => {
    if (readerRef.current?.signal !== signal) return;
    readerRef.current = null;
    setRunning(false);
  }, [setRunning]);

  /** Show what is saved in `cid`, unless the reading was left. */
  const showSaved = useCallback(async (cid: number, signal: AbortSignal) => {
    const { conversation, source, onFarOpened } = latest.current;
    // A reading that was left hands nothing up: it is not the chat on screen.
    const opened = (open: HubChatOpen) => !signal.aborted && onFarOpened?.(open);
    const { messages: thread, ...said } = await loadSavedThread(cid, conversation ?? null, source, opened);
    if (signal.aborted) return;
    messagesRef.current = thread;
    setMessages(thread);
    setBeside(said);
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
        source: latest.current.source,
        signal,
        setMessages: unlessLeft(setMessages),
        mkAssistantMessage,
        timingTracker,
        setCurrentStreamingAssistantMessageId: unlessLeft(setCurrentStreamingAssistantMessageId),
        onSystemWarning: (message, action) => latest.current.onSystemWarning?.(message, action),
        setPreviews: unlessLeft(setPreviews),
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
    setEndedRun(outcome.info);
    latest.current.onConversationChanged?.(cid);
    if (outcome.error) latest.current.onError?.(outcome.error);
  }, [endReading, showSaved, timingTracker]);

  /** Learn whether a run is live in `cid`, show its rows, then read the run. */
  const open = useCallback(async (cid: number, signal: AbortSignal) => {
    openingRef.current = true;
    unloadedRef.current = false;
    unaskedRef.current = false;
    setIsLoading(true);
    try {
      let live: Awaited<ReturnType<typeof liveRunFor>>;
      let unasked: Error | null = null;
      try {
        live = await liveRunFor(cid, latest.current.source);
      } catch (error) {
        unasked = error as Error;
      }
      if (signal.aborted) return;
      try {
        await showSaved(cid, signal);
      } catch (error) {
        if (signal.aborted) return;
        unloadedRef.current = true;
        messagesRef.current = [];
        setMessages([]);
        setBeside(NOTHING_BESIDE);
        const reason = (error as Error).message;
        latest.current.onError?.(new Error(`${NOT_LOADED} ${reason}`));
        return;
      }
      if (signal.aborted) return;
      openingRef.current = false;
      setIsLoading(false);
      if (unasked) {
        unaskedRef.current = true;
        latest.current.onError?.(unasked);
      }
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

  /**
   * Whether a send may go in `cid`: false when its rows are not loaded, or
   * when opening could not learn whether a run is live and asking again
   * finds one (which is then drawn) or fails. Says why when it is false.
   */
  const clearToSend = useCallback(async (cid: number): Promise<boolean> => {
    const refuse = (message: string) => {
      latest.current.onError?.(new Error(message));
      return false;
    };
    if (unloadedRef.current) return refuse(NOT_LOADED);
    if (!unaskedRef.current) return true;
    let live: Awaited<ReturnType<typeof liveRunFor>>;
    try {
      live = await liveRunFor(cid, latest.current.source);
    } catch (error) {
      return refuse(`${UNKNOWN_LIVE} ${(error as Error).message}`);
    }
    unaskedRef.current = false;
    if (!live) return true;
    void follow(cid, live.id, beginReading());
    return refuse(STILL_RUNNING);
  }, [beginReading, follow]);

  const systemPrompt = inputs.conversation?.system_prompt;
  const source = inputs.source ?? 'this';
  useEffect(() => {
    if (conversationId === undefined) {
      openingRef.current = false;
      setMessages([]);
      setBeside(NOTHING_BESIDE);
      setIsLoading(false);
      return undefined;
    }
    const adopted = adoptRef.current === conversationId;
    adoptRef.current = null;
    if (!adopted) void open(conversationId, beginReading());
    return stopReading;
  }, [conversationId, source, systemPrompt, open, beginReading, stopReading]);

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
    ...beside,
    isRunning,
    isLoading,
    endedRun,
    previews,
    timingTracker,
    currentStreamingAssistantMessageId,
    /** The run being read here, if the daemon has accepted it. */
    liveRunId: () => runRef.current,
    /** A send created `cid`: its opening must not interrupt the send's reading. */
    adopt: (cid: number) => {
      adoptRef.current = cid;
    },
    beginSend,
    clearToSend,
    endReading,
    showSaved,
    follow,
  };
}
