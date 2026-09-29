/**
 * Whether the Remote panel asked for chat to go to the connected machine,
 * and the name it gave for the model there.
 *
 * Returned together because they travel together: the far machine resolves
 * its own model names and this one has no catalog for them, so the name is
 * part of the routing decision rather than a detail of it. A blank field
 * reads as no name at all, which the send path refuses rather than turning
 * into the empty model the far proxy answers `404 Model '' not found`.
 *
 * Exported for the tests: it is the whole link between the Remote panel and
 * the request body, and the defect it exists for was that link being absent.
 */
export function askTheRemote(): { remote: boolean; model?: string } {
  const remote = getRemoteState();
  if (!remote.useForChat || remote.status?.connected == null) return { remote: false };
  return { remote: true, model: remote.chatModel.trim() || undefined };
}

/**
 * The chat runtime: an `ExternalStoreRuntime` over the messages
 * `useRunReader` holds, drawn from runs the daemon owns.
 *
 * A send starts a run (`PUT /api/runs/{id}?kind=agent`) under an id minted
 * here, in a conversation that exists first, and reads it. The daemon saves
 * the user's message when the run starts and the reply when it ends; the
 * page saves no turn itself. Stop cancels the run; leaving only stops
 * reading it.
 *
 * @module useGglibRuntime
 */

import { useRef, useEffect } from 'react';
import { agentOverridesToWire, reasoningOverridesToWire } from '../../services/agentOverrides';
import {
  useExternalStoreRuntime,
  useExternalMessageConverter,
  type AppendMessage,
} from '@assistant-ui/react';
import type { GglibMessage, GglibContent } from '../../types/messages';
import { mkUserMessage } from '../../types/messages';
import { getTransport } from '../../services/transport';
import { getRemoteState } from '../../services/remoteRegistry';
import { DEFAULT_SYSTEM_PROMPT } from '../../constants/prompts';
import {
  buildThreadMessages,
  type ThreadConversation,
} from '../useChatPersistence/buildThreadMessages';
import type { ReasoningTimingTracker } from './reasoningTiming';
import { buildRunRequest, mintRunId } from './runRequest';
import { savedRowId } from './savedRows';
import { useRunReader } from './useRunReader';

export interface UseGglibRuntimeOptions {
  conversationId?: number;
  /** The open conversation, whose system prompt heads its thread. */
  conversation?: ThreadConversation | null;
  selectedServerPort?: number;
  maxToolIterations?: number;
  onError?: (error: Error) => void;
  /**
   * Called for each non-fatal `system_warning` the loop emits — an upstream
   * 503 being retried, a tool-call batch being trimmed. Unlike `onError` this
   * does not mean the turn failed: the loop is still running.
   */
  onSystemWarning?: (message: string, suggestedAction?: string | null) => void;
  /**
   * Whether the active model supports tool/function calling: `false` strips
   * tools; `null` / `undefined` is unknown and treated as supported.
   */
  supportsToolCalls?: boolean | null;
  /**
   * Called with a conversation's id once a send created it, and once a run
   * in it has saved its reply: the conversation list is the caller's.
   */
  onConversationChanged?: (conversationId: number) => void;
}

export interface UseGglibRuntimeReturn {
  runtime: ReturnType<typeof useExternalStoreRuntime>;
  messages: GglibMessage[];
  setMessages: React.Dispatch<React.SetStateAction<GglibMessage[]>>;
  isRunning: boolean;
  /** Whether the open conversation's saved rows are still loading. */
  isLoading: boolean;
  timingTracker: ReasoningTimingTracker;
  currentStreamingAssistantMessageId: string | null;
}

export function useGglibRuntime(options: UseGglibRuntimeOptions = {}): UseGglibRuntimeReturn {
  const { conversationId, selectedServerPort, maxToolIterations, onError, supportsToolCalls } = options;
  const reader = useRunReader(conversationId, options);
  const { messages, setMessages, messagesRef, isRunning } = reader;

  const convertedMessages = useExternalMessageConverter({
    messages,
    callback: (m: GglibMessage) => m, // Already ThreadMessageLike
    isRunning,
    joinStrategy: 'none', // one message per loop iteration
  });

  const runtimeRef = useRef<ReturnType<typeof useExternalStoreRuntime> | null>(null);
  // Stop pressed before the daemon accepted the run: cancel it once it has.
  const stopAskedRef = useRef(false);
  // `cancelRun` puts back the messages it held when Stop was pressed, a tick
  // later. The run's end brings what the daemon saved; that copy must not
  // land on top of it.
  const skipResyncRef = useRef(false);

  /**
   * Send `content` after `base`: create the conversation if there is none,
   * start the run and read it. `replaceFrom` is the saved row an edit or a
   * regenerate replaces, with every later one; the daemon deletes them only
   * once it accepts the run, so a refusal changes nothing. Refused here
   * while a run is live, or while opening has not learned whether one is.
   */
  const start = async (
    base: GglibMessage[],
    content: GglibContent,
    { replaceFrom, giveBack = true }: { replaceFrom?: number; giveBack?: boolean } = {},
  ) => {
    // Read once: the guard and the body must agree about where this goes.
    const destination = askTheRemote();
    // A remote turn has no local server to select; the daemon takes the
    // tunnel's port and the stored key.
    if (!selectedServerPort && !destination.remote) {
      onError?.(new Error('No server selected. Please serve a model first.'));
      return;
    }
    const signal = reader.beginSend();
    if (!signal) return;
    stopAskedRef.current = false;
    let cid = conversationId;
    try {
      if (cid === undefined) {
        cid = await getTransport().createConversation({
          title: 'New Chat',
          modelId: null,
          systemPrompt: DEFAULT_SYSTEM_PROMPT,
        });
        reader.adopt(cid);
        const created = { id: cid, system_prompt: DEFAULT_SYSTEM_PROMPT, created_at: new Date().toISOString() };
        base = buildThreadMessages([], created, cid) as GglibMessage[];
        options.onConversationChanged?.(cid);
      }
      const history = [...base, mkUserMessage(content, { conversationId: cid, turnId: crypto.randomUUID() })];
      const request = buildRunRequest({
        messages: history,
        conversationId: cid,
        replaceFrom,
        selectedServerPort,
        config: {
          ...(maxToolIterations !== undefined && { max_iterations: maxToolIterations }),
          // Per-chat limits from the Tools popover, read fresh per send.
          ...agentOverridesToWire(),
        },
        reasoning: reasoningOverridesToWire(),
        supportsToolCalls,
        ...destination,
      });
      messagesRef.current = history;
      setMessages(history);
      const runId = mintRunId();
      await getTransport().startAgentRun(runId, request);
      if (stopAskedRef.current) {
        await getTransport().cancelRun(runId).catch((error: Error) => onError?.(error));
      }
      if (!signal.aborted) await reader.follow(cid, runId, signal);
    } catch (error) {
      if (signal.aborted) return;
      reader.endReading(signal);
      // Nothing was started and nothing changed: show what is saved, and
      // hand the text of a send or an edit back to the composer.
      if (cid !== undefined) await reader.showSaved(cid, signal).catch(() => {});
      const [only, ...more] = typeof content === 'string' ? [{ type: 'text', text: content } as const] : content;
      if (giveBack && more.length === 0 && only?.type === 'text') {
        runtimeRef.current?.thread.composer.setText(only.text);
      }
      onError?.(error as Error);
    }
  };

  const runtime = useExternalStoreRuntime({
    messages: convertedMessages,
    isRunning,
    setMessages: (newMessages) => {
      if (skipResyncRef.current) {
        skipResyncRef.current = false;
        return;
      }
      setMessages([...newMessages] as GglibMessage[]); // Convert from readonly
    },

    onNew: async (msg: AppendMessage) => {
      await start(messagesRef.current, msg.content as GglibContent);
    },

    // Edit and resend: the run replaces the edited row and everything after
    // it with the new message.
    onEdit: async (msg: AppendMessage) => {
      const current = messagesRef.current;
      const parent = msg.parentId === null ? -1 : current.findIndex((m) => m.id === msg.parentId);
      if (msg.parentId !== null && parent === -1) return;
      const rowId = savedRowId(current[parent + 1]);
      await start(current.slice(0, parent + 1), msg.content as GglibContent, {
        replaceFrom: rowId ?? undefined,
      });
    },

    // Regenerate: the run replaces the question and its reply with the
    // question again, so it is held once.
    onReload: async (parentId: string | null) => {
      const current = messagesRef.current;
      let at = parentId === null ? -1 : current.findIndex((m) => m.id === parentId);
      while (at >= 0 && current[at].role !== 'user') at--;
      if (at < 0) return;
      const rowId = savedRowId(current[at]);
      await start(current.slice(0, at), current[at].content as GglibContent, {
        replaceFrom: rowId ?? undefined,
        giveBack: false,
      });
    },

    // Stop: cancel the run. Its end, and what it saved, arrive as they would.
    onCancel: async () => {
      skipResyncRef.current = true;
      const runId = reader.liveRunId();
      if (!runId) {
        stopAskedRef.current = true;
        return;
      }
      try {
        await getTransport().cancelRun(runId);
      } catch (error) {
        onError?.(error as Error);
      }
    },
  });
  useEffect(() => {
    runtimeRef.current = runtime;
  }, [runtime]);

  return {
    runtime,
    messages,
    setMessages,
    isRunning,
    isLoading: reader.isLoading,
    timingTracker: reader.timingTracker,
    currentStreamingAssistantMessageId: reader.currentStreamingAssistantMessageId,
  };
}

// Re-export types for convenience
export type { GglibMessage, GglibContent };
