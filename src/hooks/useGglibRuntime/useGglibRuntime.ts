/**
 * Custom runtime hook using ExternalStoreRuntime.
 * 
 * Manages message state externally, allowing one assistant message per
 * agentic loop iteration without overwriting previous messages.
 * 
 * @module useGglibRuntime
 */

import { useState, useRef, useEffect } from 'react';
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
  
  // Track which assistant message is currently streaming (for live timer)
  const [currentStreamingAssistantMessageId, setCurrentStreamingAssistantMessageId] = useState<string | null>(null);
  // Ref to access current streaming message ID without stale closure in onCancel
  const currentStreamingAssistantMessageIdRef = useRef<string | null>(null);
  
  // Sync ref with state
  useEffect(() => {
    currentStreamingAssistantMessageIdRef.current = currentStreamingAssistantMessageId;
  }, [currentStreamingAssistantMessageId]);

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

  // Create runtime with external message management
  const runtime = useExternalStoreRuntime({
    messages: convertedMessages,
    isRunning,
    setMessages: (newMessages) => {
      setMessages([...newMessages] as GglibMessage[]); // Convert from readonly
    },

    // User sends a new message
    onNew: async (msg: AppendMessage) => {
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

      // Generate unique turn ID
      const turnId = crypto.randomUUID();

      // Add user message
      const userMessage = mkUserMessage(msg.content as GglibContent, {
        conversationId,
        turnId,
      });
      
      // Capture messages WITH the new user message to avoid race condition
      // (messagesRef.current won't be updated until after the next render)
      const messagesWithUserMessage = [...messagesRef.current, userMessage];
      setMessages(prev => [...prev, userMessage]);

      // Start generation
      setIsRunning(true);

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
          mkAssistantMessage: (custom) => mkAssistantMessage(custom),
          timingTracker,
          setCurrentStreamingAssistantMessageId,
        });
      } catch (error) {
        if (error instanceof Error && error.name === 'AbortError') {
          console.log('Generation aborted');
        } else {
          console.error('Error in agentic loop:', error);
          onError?.(error as Error);
        }
      } finally {
        // Only clear state if we weren't explicitly cancelled via onCancel
        // (onCancel sets abortControllerRef.current to null first)
        if (abortControllerRef.current !== null) {
          setIsRunning(false);
          setCurrentStreamingAssistantMessageId(null);
          abortControllerRef.current = null;
        }
      }
    },

    // User edits a message (regenerate)
    onEdit: async (msg: AppendMessage) => {
      // Find the edited message
      const editedIdx = messages.findIndex(m => m.id === msg.parentId);
      if (editedIdx === -1) return;

      // Remove all messages after the edited one
      const newMessages = messages.slice(0, editedIdx);
      setMessages(newMessages);

      // Call onNew to continue generation
      await runtime.thread.append(msg);
    },

    // User reloads conversation (not supported yet)
    onReload: async (_parentId: string | null) => {
      // Reload not implemented yet
      console.warn('Reload not implemented');
    },

    // User cancels generation
    onCancel: async () => {
      if (abortControllerRef.current) {
        abortControllerRef.current.abort();
        abortControllerRef.current = null;
      }
      
      // Get current streaming message ID from ref (avoids stale closure)
      const streamingId = currentStreamingAssistantMessageIdRef.current;
      
      // Atomic state update: mark message as stopped, clear streaming ID, set not running
      // This ensures assistant-ui derives the correct status from the message
      if (streamingId) {
        setMessages(prev => 
          prev.map(m => {
            if (m.id !== streamingId) return m;
            
            // Mark this message as [Stopped]
            const updatedContent = Array.isArray(m.content) 
              ? [
                  ...m.content,
                  {
                    type: 'text' as const,
                    text: '\n\n[Stopped]',
                  },
                ]
              : [{
                  type: 'text' as const,
                  text: '[Stopped]',
                }];
            
            return {
              ...m,
              content: updatedContent as GglibContent,
              // Explicit status tells assistant-ui the message is complete (not running)
              // This is critical: assistant-ui derives isRunning from last message's status
              status: { type: 'complete', reason: 'stop' },
            };
          })
        );
      }
      
      setIsRunning(false);
      setCurrentStreamingAssistantMessageId(null);
    },
  });

  return {
    runtime,
    messages,
    setMessages,
    isRunning,
    timingTracker,
    currentStreamingAssistantMessageId,
  };
}

// Re-export types for convenience
export type { GglibMessage, GglibContent };
