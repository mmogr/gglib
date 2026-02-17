/**
 * Custom runtime hook using ExternalStoreRuntime.
 * 
 * Manages message state externally, allowing one assistant message per
 * agentic loop iteration without overwriting previous messages.
 * 
 * @module useGglibRuntime
 */

import { useState, useRef, useEffect, useCallback } from 'react';
import { appLogger } from '../../services/platform';
import {
  useExternalStoreRuntime,
  useExternalMessageConverter,
  type AppendMessage,
} from '@assistant-ui/react';
import type { GglibMessage, GglibContent } from '../../types/messages';
import { mkUserMessage, mkAssistantMessage } from '../../types/messages';
import { runAgenticLoop } from './runAgenticLoop';
import { ReasoningTimingTracker } from './reasoningTiming';
import { performanceClock } from './clock';

export interface UseGglibRuntimeOptions {
  conversationId?: number;
  selectedServerPort?: number;
  maxToolIterations?: number;
  maxStagnationSteps?: number;
  onError?: (error: Error) => void;
}

export interface UseGglibRuntimeReturn {
  runtime: ReturnType<typeof useExternalStoreRuntime>;
  messages: GglibMessage[];
  setMessages: React.Dispatch<React.SetStateAction<GglibMessage[]>>;
  isRunning: boolean;
  timingTracker: ReasoningTimingTracker;
  currentStreamingAssistantMessageId: string | null;
  /** Set extra custom metadata to merge into the next user message */
  setNextMessageMeta: (meta: Partial<import('../../types/messages').GglibMessageCustom>) => void;
}

/**
 * Custom runtime hook using ExternalStoreRuntime.
 * 
 * Creates one assistant message per agentic loop iteration, preventing
 * message overwriting. Uses external message state management.
 */
export function useGglibRuntime(options: UseGglibRuntimeOptions = {}): UseGglibRuntimeReturn {
  const {
    conversationId,
    selectedServerPort,
    maxToolIterations,
    maxStagnationSteps,
    onError,
  } = options;

  // Message state managed externally
  const [messages, setMessages] = useState<GglibMessage[]>([]);
  
  // Ref to avoid stale closures in async callbacks
  const messagesRef = useRef(messages);
  useEffect(() => {
    messagesRef.current = messages;
  }, [messages]);

  // Abort controller for cancellation
  const abortControllerRef = useRef<AbortController | null>(null);
  const [isRunning, setIsRunning] = useState(false);

  // Extra metadata to merge into the next user message (e.g. isVoice)
  const nextMessageMetaRef = useRef<Partial<import('../../types/messages').GglibMessageCustom>>({});
  const setNextMessageMeta = useCallback((meta: Partial<import('../../types/messages').GglibMessageCustom>) => {
    nextMessageMetaRef.current = meta;
  }, []);
  
  // Track which assistant message is currently streaming (for live timer)
  const [currentStreamingAssistantMessageId, setCurrentStreamingAssistantMessageId] = useState<string | null>(null);

  // Timing tracker for reasoning duration (persists across renders)
  const timingTrackerRef = useRef(new ReasoningTimingTracker(performanceClock));
  const timingTracker = timingTrackerRef.current;

  // Clear timing data when switching conversations (prevent memory leak)
  useEffect(() => {
    timingTracker.clearAll();
  }, [conversationId, timingTracker]);

  // Convert messages with joinStrategy: 'none' to prevent merging iterations
  const convertedMessages = useExternalMessageConverter({
    messages,
    callback: (m: GglibMessage) => m, // Already ThreadMessageLike
    isRunning,
    joinStrategy: 'none', // Critical: preserves per-iteration boundaries
  });

  /**
   * Shared generation logic used by both onNew and onEdit.
   *
   * Takes a base message history and a new user message, appends the user
   * message, synchronises messagesRef, and runs the agentic loop.
   */
  const startGeneration = async (
    baseMessages: GglibMessage[],
    userMessage: GglibMessage,
    extraMeta: Partial<import('../../types/messages').GglibMessageCustom> = {},
  ) => {
    // Validate server selection
    if (!selectedServerPort) {
      const error = new Error('No server selected. Please serve a model first.');
      onError?.(error);
      return;
    }

    // Abort any existing generation
    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
    }

    // Create new abort controller
    const abortController = new AbortController();
    abortControllerRef.current = abortController;

    // Build the full message list with the new user message
    const messagesWithUserMessage = [...baseMessages, userMessage];

    // Synchronise ref immediately so async callbacks see the correct history
    // (the useEffect sync won't fire until after the next render)
    messagesRef.current = messagesWithUserMessage;
    setMessages(messagesWithUserMessage);

    // Start generation
    setIsRunning(true);

    // Generate unique turn ID
    const turnId = crypto.randomUUID();

    try {
      // Run agentic loop - creates assistant messages as needed
      await runAgenticLoop({
        turnId,
        getMessages: () => messagesWithUserMessage,
        setMessages,
        selectedServerPort,
        maxToolIterations,
        maxStagnationSteps,
        abortSignal: abortController.signal,
        conversationId,
        mkAssistantMessage: (custom) => mkAssistantMessage({ ...custom, ...extraMeta }),
        timingTracker,
        setCurrentStreamingAssistantMessageId,
      });
    } catch (error) {
      if (error instanceof Error && error.name === 'AbortError') {
        appLogger.debug('hook.runtime', 'Generation aborted');
      } else {
        appLogger.error('hook.runtime', 'Error in agentic loop', { error });
        onError?.(error as Error);
      }
    } finally {
      setIsRunning(false);
      setCurrentStreamingAssistantMessageId(null);
      abortControllerRef.current = null;
    }
  };

  // Create runtime with external message management
  const runtime = useExternalStoreRuntime({
    messages: convertedMessages,
    isRunning,
    setMessages: (newMessages) => {
      setMessages([...newMessages] as GglibMessage[]); // Convert from readonly
    },

    // User sends a new message
    onNew: async (msg: AppendMessage) => {
      // Drain any one-shot metadata (e.g. isVoice) queued for this message
      const extraMeta = nextMessageMetaRef.current;
      nextMessageMetaRef.current = {};

      const userMessage = mkUserMessage(msg.content as GglibContent, {
        conversationId,
        turnId: crypto.randomUUID(),
        ...extraMeta,
      });
      await startGeneration(messagesRef.current, userMessage, extraMeta);
    },

    // User edits a message (regenerate)
    onEdit: async (msg: AppendMessage) => {
      // msg.parentId is the ID of the message *before* the edited one
      // (the branch-point parent in @assistant-ui/react's tree model).
      const parentIdx = messages.findIndex(m => m.id === msg.parentId);
      if (parentIdx === -1) return;

      // Keep history up to and including the parent; drop the old edited
      // message and everything after it.
      const baseMessages = messages.slice(0, parentIdx + 1);

      const userMessage = mkUserMessage(msg.content as GglibContent, {
        conversationId,
        turnId: crypto.randomUUID(),
      });
      await startGeneration(baseMessages, userMessage);
    },

    // User reloads conversation (not supported yet)
    onReload: async (_parentId: string | null) => {
      // Reload not implemented yet
      appLogger.warn('hook.runtime', 'Reload not implemented');
    },

    // User cancels generation
    onCancel: async () => {
      if (abortControllerRef.current) {
        abortControllerRef.current.abort();
        abortControllerRef.current = null;
      }
      setIsRunning(false);
    },
  });

  return {
    runtime,
    messages,
    setMessages,
    isRunning,
    timingTracker,
    currentStreamingAssistantMessageId,
    setNextMessageMeta,
  };
}

// Re-export types for convenience
export type { GglibMessage, GglibContent };
