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
 * A send carries the composer's images by the ids their uploads answered
 * (`imageAttachments`), and so does every user message of the history it
 * sends. A far chat (`source: 'far'`) is the far machine's: a send there is
 * its text and images, uploaded to that machine, which runs and saves it,
 * and it offers no edit, no regenerate and no new chat. A chat with a far
 * model (`pairedModel`) is this machine's, run here on that machine's model:
 * its runs name the model by its machine, and a conversation made for it
 * keeps that model.
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
import { getTransport, type ChatSource } from '../../services/transport';
import type { ModelRef } from '../../types/generated/ModelRef';
import { DEFAULT_SYSTEM_PROMPT } from '../../constants/prompts';
import {
  buildThreadMessages,
  type ThreadConversation,
} from '../useChatPersistence/buildThreadMessages';
import type { ReasoningTimingTracker } from './reasoningTiming';
import { buildRunRequest, mintRunId } from './runRequest';
import { imageStoreOf, runsOf, turnText } from './chatSource';
import { useImageAttachments, type SentImage } from './imageAttachments';
import type { Downscale } from './imagePrep';
import { codeOf, sendRefusal } from './imageRefusals';
import { giveDraftBack, imagesOf, unsentImage } from './turnImages';
import { savedRowId } from './savedRows';
import { useRunReader } from './useRunReader';

export interface UseGglibRuntimeOptions {
  conversationId?: number;
  /** Whose chat it is; this machine's when absent. */
  source?: ChatSource;
  /** The open conversation, whose system prompt heads its thread. */
  conversation?: ThreadConversation | null;
  selectedServerPort?: number;
  /** The paired machine's model the chat is with, in place of a server here. */
  pairedModel?: ModelRef;
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
  /** Tell the person why an image was not added: shown at once, as a toast. */
  onImageRefused?: (sentence: string) => void;
  /** Makes an image too large to send smaller; the browser's canvas by default. */
  downscaleImage?: Downscale;
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
  const { conversationId, selectedServerPort, pairedModel, maxToolIterations, onError, supportsToolCalls } = options;
  const source = options.source ?? 'this';
  const far = source === 'far';
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

  const images = useImageAttachments(source, options.onImageRefused, options.downscaleImage);
  /** Put a draft back in the composer, text and images, rather than lose it. */
  const giveBackDraft = (content: GglibContent, attached: readonly SentImage[]) =>
    giveDraftBack(runtimeRef.current?.thread.composer, content, attached, imageStoreOf(source).blob);

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
    attached: readonly SentImage[],
    { replaceFrom, giveBack = true }: { replaceFrom?: number; giveBack?: boolean } = {},
  ) => {
    const handBack = () => giveBack && giveBackDraft(content, attached);
    // A far model's turn has no local server to select; the daemon takes the
    // tunnel's port and the stored key. A far chat's model is chosen there.
    if (!far && !selectedServerPort && !pairedModel) {
      onError?.(new Error('No server selected. Please serve a model first.'));
      return;
    }
    if (far && conversationId === undefined) {
      onError?.(new Error('A chat on the other machine is started there.'));
      handBack();
      return;
    }
    // An image whose upload failed: nothing is sent, and the draft goes back.
    const unsent = unsentImage(attached);
    if (unsent) {
      onError?.(new Error(unsent));
      handBack();
      return;
    }
    // Never from a conversation that is not loaded, or into a run that may
    // still be going: the text goes back to the composer.
    if (conversationId !== undefined && !(await reader.clearToSend(conversationId))) {
      handBack();
      return;
    }
    const signal = reader.beginSend();
    if (!signal) {
      // A run is live, or opening has not learned whether one is: nothing
      // is sent, and the text goes back rather than being lost.
      handBack();
      return;
    }
    stopAskedRef.current = false;
    let cid = conversationId;
    try {
      if (cid === undefined) {
        cid = await getTransport().createConversation({
          title: 'New Chat',
          modelId: null,
          systemPrompt: DEFAULT_SYSTEM_PROMPT,
          model: pairedModel ?? null,
        });
        reader.adopt(cid);
        const created = { id: cid, system_prompt: DEFAULT_SYSTEM_PROMPT, created_at: new Date().toISOString() };
        base = buildThreadMessages([], created, cid) as GglibMessage[];
        options.onConversationChanged?.(cid);
      }
      const asked = mkUserMessage(content, { conversationId: cid, turnId: crypto.randomUUID() });
      const history = [...base, attached.length > 0 ? { ...asked, attachments: attached } : asked];
      const request = far ? null : buildRunRequest({
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
        far: pairedModel,
      });
      messagesRef.current = history;
      setMessages(history);
      const runId = mintRunId();
      if (request) await getTransport().startAgentRun(runId, request);
      else await getTransport().addFarTurn(cid, runId, turnText(content), attached.map((image) => image.id));
      if (stopAskedRef.current) {
        await runsOf(source).cancelRun(runId).catch((error: Error) => onError?.(error));
      }
      if (!signal.aborted) await reader.follow(cid, runId, signal);
    } catch (error) {
      if (signal.aborted) return;
      reader.endReading(signal);
      // Nothing was started and nothing changed: show what is saved, and
      // hand the text and images of a send or an edit back to the composer.
      if (cid !== undefined) await reader.showSaved(cid, signal).catch(() => {});
      const refused = sendRefusal(error, far, attached.length > 0);
      // A store that lost an image: added back, it is uploaded again.
      if (codeOf(error) === 'attachment_not_found') images.forget(attached.flatMap((image) => image.file ?? []));
      handBack();
      onError?.(refused ? new Error(refused) : (error as Error));
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
    adapters: { attachments: images },

    onNew: async (msg: AppendMessage) => {
      await start(messagesRef.current, msg.content as GglibContent, imagesOf(msg));
    },

    // Edit and resend: the run replaces the edited row and everything after
    // it with the new message. Not on a far chat.
    onEdit: far ? undefined : async (msg: AppendMessage) => {
      const current = messagesRef.current;
      const parent = msg.parentId === null ? -1 : current.findIndex((m) => m.id === msg.parentId);
      if (msg.parentId !== null && parent === -1) return;
      const rowId = savedRowId(current[parent + 1]);
      await start(current.slice(0, parent + 1), msg.content as GglibContent, imagesOf(msg), {
        replaceFrom: rowId ?? undefined,
      });
    },

    // Regenerate: the run replaces the question and its reply with the
    // question again, so it is held once. Not on a far chat.
    onReload: far ? undefined : async (parentId: string | null) => {
      const current = messagesRef.current;
      let at = parentId === null ? -1 : current.findIndex((m) => m.id === parentId);
      while (at >= 0 && current[at].role !== 'user') at--;
      if (at < 0) return;
      const rowId = savedRowId(current[at]);
      await start(current.slice(0, at), current[at].content as GglibContent, imagesOf(current[at]), {
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
        await runsOf(source).cancelRun(runId);
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
