/**
 * Chat message persistence hook for ExternalStoreRuntime.
 * 
 * Handles hydration and persistence of messages to/from the database.
 * Works with external message state (not threadRuntime internals).
 * 
 * @module useChatPersistence
 */

import { useEffect, useRef, useState } from 'react';
import { appLogger } from '../services/platform';
import type { ThreadMessageLike } from '@assistant-ui/react';
import { getMessages, saveMessage } from '../services/clients/chat';
import type { ChatMessage } from '../services/clients/chat';
import { threadMessageToTranscriptMarkdown } from '../utils/messages';
import type { ReasoningTimingTracker } from './useGglibRuntime/reasoningTiming';

/**
 * Extract database ID from message metadata.
 */
function getDbId(m: ThreadMessageLike): number | undefined {
  return (m.metadata as any)?.custom?.dbId;
}

/**
 * Create a stable digest for detecting message content changes.
 * Includes timingFinalized flag to trigger final persist after streaming.
 */
function digestMessage(m: ThreadMessageLike): string {
  const timingFinalized = (m.metadata as any)?.custom?.timingFinalized;
  return JSON.stringify({ role: m.role, content: m.content, timingFinalized });
}



export interface UseChatPersistenceOptions {
  /** Active conversation ID */
  activeConversationId: number | null;
  /** System prompt for the active conversation */
  systemPrompt?: string | null;
  /** Created timestamp for system message */
  conversationCreatedAt?: string;
  /** Current messages array */
  messages: readonly ThreadMessageLike[];
  /** Setter for messages array */
  setMessages: React.Dispatch<React.SetStateAction<ThreadMessageLike[]>>;
  /** Callback to sync conversations list (silent refresh after saves) */
  syncConversations: (options?: { preferredId?: number | null; silent?: boolean }) => Promise<void>;
  /** Error callback */
  setChatError: (error: string | null) => void;
  /** Optional timing tracker for reasoning duration injection */
  timingTracker?: ReasoningTimingTracker;
}

export interface UseChatPersistenceResult {
  /** Whether messages are currently being loaded from DB */
  isLoading: boolean;
  /** Whether a persist operation is in progress */
  isPersisting: boolean;
}

/**
 * Hook that handles message persistence to/from the database.
 * 
 * Responsibilities:
 * - Hydrates messages from DB when conversation changes
 * - Persists new messages as they're created
 * - Updates existing messages when content changes (throttled)
 * - Tracks message IDs to prevent duplicate saves
 */
export function useChatPersistence({
  activeConversationId,
  systemPrompt,
  conversationCreatedAt,
  messages,
  setMessages,
  syncConversations,
  setChatError,
  timingTracker,
}: UseChatPersistenceOptions): UseChatPersistenceResult {
  // Track which message IDs have been persisted (runtime ID -> DB ID)
  const persistedByMessageId = useRef(new Map<string, number>());
  
  // Track last saved digest for each message (for update detection)
  const lastDigestByMessageId = useRef(new Map<string, string>());
  
  // Throttle timers for updates (per message ID)
  const updateTimers = useRef(new Map<string, number>());
  
  // Cleanup timers for delayed tracker clearing (per message ID)
  const cleanupTimers = useRef(new Map<string, number>());
  
  // Track messages currently being processed (prevents concurrent operations)
  const processingRef = useRef(new Set<string>());
  
  // Loading and persisting state
  const [isLoading, setIsLoading] = useState(false);
  const isPersistingRef = useRef(false);

  // Clear caches when switching conversations
  useEffect(() => {
    persistedByMessageId.current.clear();
    lastDigestByMessageId.current.clear();
    updateTimers.current.forEach((t) => window.clearTimeout(t));
    updateTimers.current.clear();
    cleanupTimers.current.forEach((t) => window.clearTimeout(t));
    cleanupTimers.current.clear();
    updateTimers.current.clear();
  }, [activeConversationId]);

  // Effect 1: Hydrate messages from DB when conversation changes
  useEffect(() => {
    if (!activeConversationId) {
      setIsLoading(false);
      return;
    }

    let cancelled = false;
    setIsLoading(true);
    setChatError(null);

    const hydrate = async () => {
      try {
        const dbMessages = await getMessages(activeConversationId);
        if (cancelled) return;

        // Build system prompt message if exists
        const prompt = systemPrompt?.trim();
        const systemMessage: ThreadMessageLike[] = prompt
          ? [{
              id: `system-${activeConversationId}`,
              role: 'system' as const,
              content: [{ type: 'text' as const, text: prompt }],
              createdAt: conversationCreatedAt ? new Date(conversationCreatedAt) : new Date(),
            }]
          : [];

        // Convert DB messages to ThreadMessageLike format
        const loadedMessages: ThreadMessageLike[] = [
          ...systemMessage,
          ...dbMessages.map<ThreadMessageLike>((msg) => ({
            id: `db-${msg.id}`,
            role: msg.role as 'user' | 'assistant',
            content: msg.content,
            createdAt: new Date(msg.created_at),
            metadata: {
              custom: {
                dbId: msg.id,
                conversationId: activeConversationId,
              },
            },
          })),
        ];

        // Track all loaded messages as persisted
        dbMessages.forEach((msg) => {
          const runtimeId = `db-${msg.id}`;
          persistedByMessageId.current.set(runtimeId, msg.id);
          lastDigestByMessageId.current.set(
            runtimeId,
            JSON.stringify({ role: msg.role, content: msg.content })
          );
        });

        // Set messages
        setMessages(loadedMessages);
      } catch (error) {
        if (!cancelled) {
          setChatError(error instanceof Error ? error.message : String(error));
        }
      } finally {
        if (!cancelled) {
          setIsLoading(false);
        }
      }
    };

    hydrate();
    return () => { cancelled = true; };
  }, [
    activeConversationId,
    systemPrompt,
    conversationCreatedAt,
    setMessages,
    setChatError,
  ]);

  // Effect 2: Persist new/changed messages
  useEffect(() => {
    const conversationId = activeConversationId;
    if (!conversationId) return;

    // Process each message
    for (const m of messages) {
      // Skip messages without IDs or system messages
      if (!m.id || m.role === 'system') continue;

      // Skip if already processing this message ID
      if (processingRef.current.has(m.id) || isPersistingRef.current) continue;

      const currentDigest = digestMessage(m);
      const existingDbId = getDbId(m) ?? persistedByMessageId.current.get(m.id);

      // Case 1: New message - create in DB
      if (!existingDbId) {
        // Check if message has any content
        const text = threadMessageToTranscriptMarkdown(m as any, 
          // Only inject durations for assistant messages when tracker available
          m.role === 'assistant' && timingTracker
            ? { getDurationForSegment: (msgId, idx) => timingTracker.getDurationSec(msgId, idx) }
            : undefined
        );
        if (!text.trim()) continue;

        // Check if already persisted via the ref (prevents double-save)
        if (persistedByMessageId.current.has(m.id)) continue;

        const messageId = m.id; // Capture for closure
        processingRef.current.add(messageId);
        isPersistingRef.current = true;

        (async () => {
          try {
            const dbId = await saveMessage(
              conversationId,
              m.role as ChatMessage['role'],
              text
            );

            persistedByMessageId.current.set(messageId, dbId);
            lastDigestByMessageId.current.set(messageId, currentDigest);

            await syncConversations({ silent: true });
          } catch (error) {
            appLogger.error('hook.persistence', 'Failed to persist new message', { error });
            persistedByMessageId.current.delete(messageId); // Allow retry on next render
          } finally {
            processingRef.current.delete(messageId);
            isPersistingRef.current = false;
          }
        })();
        continue;
      }

      // Case 2: Existing message - check for updates
      const prevDigest = lastDigestByMessageId.current.get(m.id);
      if (prevDigest !== currentDigest) {
        lastDigestByMessageId.current.set(m.id, currentDigest);

        // Throttle updates per message ID (prevents update on every streaming token)
        const oldTimer = updateTimers.current.get(m.id);
        if (oldTimer) window.clearTimeout(oldTimer);

        const messageId = m.id; // Capture for closure
        const timer = window.setTimeout(async () => {
          processingRef.current.add(messageId);
          isPersistingRef.current = true;
          try {
            const text = threadMessageToTranscriptMarkdown(m as any,
              // Only inject durations for assistant messages when tracker available
              m.role === 'assistant' && timingTracker
                ? { getDurationForSegment: (msgId, idx) => timingTracker.getDurationSec(msgId, idx) }
                : undefined
            );
            if (text.trim() && existingDbId) {
              await saveMessage(
                conversationId,
                m.role as ChatMessage['role'],
                text
              );
              await syncConversations({ silent: true });
              
              // Cleanup timing data after successful persist (prevent memory growth)
              // Delay by 60s to allow UI to display final duration before clearing tracker
              // Cancel any existing cleanup timer for this message (idempotent)
              const isFinalized = (m.metadata as any)?.custom?.timingFinalized;
              if (m.role === 'assistant' && isFinalized && timingTracker) {
                const existingTimer = cleanupTimers.current.get(messageId);
                if (existingTimer) {
                  window.clearTimeout(existingTimer);
                }
                const timerId = window.setTimeout(() => {
                  timingTracker.clearMessage(messageId);
                  cleanupTimers.current.delete(messageId);
                }, 60_000);
                cleanupTimers.current.set(messageId, timerId);
              }
            }
          } catch (error) {
            appLogger.error('hook.persistence', 'Failed to update message', { error });
          } finally {
            processingRef.current.delete(messageId);
            updateTimers.current.delete(messageId);
            isPersistingRef.current = false;
          }
        }, 500); // 500ms throttle

        updateTimers.current.set(m.id, timer);
      }
    }
  }, [
    activeConversationId,
    messages,
    syncConversations,
  ]);

  return {
    isLoading,
    isPersisting: isPersistingRef.current,
  };
}
