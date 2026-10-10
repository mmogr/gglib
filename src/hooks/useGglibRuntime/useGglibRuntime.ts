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
 * keeps that model. Either kind of send says the chat's Thinking choice only
 * when the caller's `thinking` gives one, and calls its `accepted` once taken;
 * and says `draw` only when the caller's `draw` gives one, likewise.
 *
 * An edit, a regenerate, Retry and Branch from here are changes the daemon
 * makes (`branchChanges`): one that would rewrite a saved reply is made on a
 * new branch of the chat, which the page opens, and the chat it leaves is
 * answered by a run that asks nothing new (`answer_saved`).
 *
 * @module useGglibRuntime
 */

import { useRef, useEffect } from 'react';
import {
  useExternalStoreRuntime,
  useExternalMessageConverter,
  type AppendMessage,
} from '@assistant-ui/react';
import type { GglibMessage, GglibContent } from '../../types/messages';
import { mkUserMessage } from '../../types/messages';
import { getTransport, type ChatSource } from '../../services/transport';
import type { ChatChange } from '../../types/generated/ChatChange';
import type { ModelRef } from '../../types/generated/ModelRef';
import type { RunInfo } from '../../types/generated/RunInfo';
import { DEFAULT_SYSTEM_PROMPT } from '../../constants/prompts';
import {
  buildThreadMessages,
  type ThreadConversation,
} from '../useChatPersistence/buildThreadMessages';
import type { ReasoningTimingTracker } from './reasoningTiming';
import { mintRunId, runBodyFor, type RunRequestOptions } from './runRequest';
import { imageStoreOf, runsOf, turnText } from './chatSource';
import { useImageAttachments, type SentImage } from './imageAttachments';
import type { Downscale } from './imagePrep';
import { codeOf, sendRefusal } from './imageRefusals';
import { giveDraftBack, imagesOf, unsentImage } from './turnImages';
import { changeAndAnswer, editOf, regenerateOf, useBranching, type Branching } from './branchChanges';
import { useRunReader, type RunReaderInputs } from './useRunReader';
import type { RunPreviews } from './runPreviews';

export interface UseGglibRuntimeOptions extends Pick<RunReaderInputs, 'onFarOpened'> {
  conversationId?: number;
  /** Whose chat it is; this machine's when absent. */
  source?: ChatSource;
  /** The open conversation, whose system prompt heads its thread. */
  conversation?: ThreadConversation | null;
  selectedServerPort?: number;
  /** The paired machine's model the chat is with, in place of a server here. */
  pairedModel?: ModelRef;
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
  /** A change was made on a new branch, `conversationId`, which is opened; `unanswered` says why its answer did not start. */
  onBranched?: (conversationId: number, unanswered?: Error) => void;
  /** Tell the person why an image was not added: shown at once, as a toast. */
  onImageRefused?: (sentence: string) => void;
  /** Makes an image too large to send smaller; the browser's canvas by default. */
  downscaleImage?: Downscale;
  /** What a send says of the chat's Thinking choice, asked at each send: nothing unless it changed, and `accepted` is called once its turn is. */
  thinking?: () => { said: RunRequestOptions['thinking']; accepted: () => void } | undefined;
  /** Whether a send says `draw`, asked at each send: something only while the Draw button is armed, and `accepted` is called once its turn is. */
  draw?: () => { accepted: () => void } | undefined;
}

export interface UseGglibRuntimeReturn {
  runtime: ReturnType<typeof useExternalStoreRuntime>;
  messages: GglibMessage[];
  setMessages: React.Dispatch<React.SetStateAction<GglibMessage[]>>;
  isRunning: boolean;
  /** Whether the open conversation's saved rows are still loading. */
  isLoading: boolean;
  /** The run last read to its end in the open conversation, as it ended; null once it is left. */
  endedRun: RunInfo | null;
  /** The frames a tool of the run being read is making, by its call: beside the messages, never in them. */
  previews: RunPreviews;
  timingTracker: ReasoningTimingTracker;
  currentStreamingAssistantMessageId: string | null;
  /** What the open chat offers of its branches, and Retry; the same object until what the daemon says of the chat changes. */
  branching: Branching;
}

export function useGglibRuntime(options: UseGglibRuntimeOptions = {}): UseGglibRuntimeReturn {
  const { conversationId, selectedServerPort, pairedModel, onError, supportsToolCalls } = options;
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

  // Whether a run can start, saying why not: a far model's turn needs no server here.
  const canRun = () => {
    if (far || selectedServerPort || pairedModel) return true;
    onError?.(new Error('No server selected. Please serve a model first.'));
    return false;
  };

  const target = { selectedServerPort, supportsToolCalls, far: pairedModel };

  /** Start a run answering the question `cid` ends in, as saved; its id. */
  const answer = async (cid: number) => {
    const thinking = options.thinking?.();
    const runId = mintRunId();
    const draw = options.draw?.();
    const body = runBodyFor(target, { conversationId: cid, messages: [], answerSaved: true, thinking: thinking?.said, draw: draw !== undefined });
    await getTransport().startAgentRun(runId, body);
    thinking?.accepted();
    draw?.accepted();
    if (stopAskedRef.current) await runsOf(source).cancelRun(runId).catch((error: Error) => onError?.(error));
    return runId;
  };

  /** Make `change` to the open chat, or answer the question it ends in (Retry). */
  const change = async (made: ChatChange | null) => {
    if (!canRun()) return;
    stopAskedRef.current = false;
    const { onConversationChanged, onBranched } = options;
    await changeAndAnswer(made, { conversationId, reader, answer, onConversationChanged, onBranched, onError });
  };

  /**
   * Send `content` after `base`: create the conversation if there is none,
   * start the run and read it. Refused here while a run is live, or while
   * opening has not learned whether one is.
   */
  const start = async (base: GglibMessage[], content: GglibContent, attached: readonly SentImage[]) => {
    const handBack = () => giveBackDraft(content, attached);
    if (!canRun()) return;
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
      const thinking = options.thinking?.();
      const draw = options.draw?.();
      const request = far ? null : runBodyFor(target, { conversationId: cid, messages: history, thinking: thinking?.said, draw: draw !== undefined });
      messagesRef.current = history;
      setMessages(history);
      const runId = mintRunId();
      if (request) await getTransport().startAgentRun(runId, request);
      else await getTransport().addFarTurn(cid, runId, turnText(content), attached.map((image) => image.id), thinking?.said, draw !== undefined);
      thinking?.accepted();
      draw?.accepted();
      if (stopAskedRef.current) await runsOf(source).cancelRun(runId).catch((error: Error) => onError?.(error));
      if (!signal.aborted) await reader.follow(cid, runId, signal);
    } catch (error) {
      if (signal.aborted) return;
      reader.endReading(signal);
      // Nothing was started and nothing changed: show what is saved, and
      // hand the text and images of the send back to the composer.
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

    // An edit of a question or a reply. One that would rewrite a saved reply
    // is made on a new branch. Not on a far chat.
    onEdit: far ? undefined : async (msg: AppendMessage) => {
      const made = editOf(msg, messagesRef.current);
      if (made instanceof Error) onError?.(made);
      else await change(made);
    },

    // Regenerate: the reply is answered again on a new branch. Not on a far chat.
    onReload: far ? undefined : async (parentId: string | null) => {
      const made = regenerateOf(parentId, messagesRef.current);
      if (made) await change(made);
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
  const branching = useBranching(reader, messagesRef, change, options.onConversationChanged);

  return {
    runtime,
    messages,
    setMessages,
    isRunning,
    isLoading: reader.isLoading,
    endedRun: reader.endedRun,
    previews: reader.previews,
    timingTracker: reader.timingTracker,
    currentStreamingAssistantMessageId: reader.currentStreamingAssistantMessageId,
    branching,
  };
}

// Re-export types for convenience
export type { GglibMessage, GglibContent };
