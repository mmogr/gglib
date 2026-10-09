/**
 * Edit, regenerate, Retry and Branch from here, as the page makes them
 * (ADR 0017). A change is posted to the daemon, which makes it in place or
 * on a new branch of the chat and says which; the chat it leaves is shown,
 * and answered by a run when the change says so. The page holds no rule of
 * when a change branches: that is the daemon's.
 *
 * A branch opens as a new chat would: the caller is told its id, and the
 * page selects it. Its answer run is started first, so the branch is read
 * with its reply already going.
 *
 * @module branchChanges
 */

import { useEffect, useMemo, useRef, type RefObject } from 'react';
import type { AppendMessage } from '@assistant-ui/react';
import { getTransport } from '../../services/transport';
import type { ChatChange } from '../../types/generated/ChatChange';
import type { ChatChanged } from '../../types/generated/ChatChanged';
import type { GglibContent, GglibMessage } from '../../types/messages';
import { turnText } from './chatSource';
import { savedRowId, type SavedView } from './savedRows';
import { imagesOf, unsentImage } from './turnImages';

/** The change an edit in the thread asks for, or why it cannot be made. */
export function editOf(msg: AppendMessage, current: GglibMessage[]): ChatChange | Error {
  const at = msg.parentId === null ? -1 : current.findIndex((m) => m.id === msg.parentId);
  const edited = msg.sourceId ? current.find((m) => m.id === msg.sourceId) : current[at + 1];
  const id = savedRowId(edited);
  if (id === null) return new Error('Only a saved message can be edited.');
  const attached = imagesOf(msg);
  const unsent = unsentImage(attached);
  if (unsent) return new Error(unsent);
  const images = attached.map((image) => image.id);
  const content = turnText(msg.content as GglibContent);
  return { kind: 'edit', message_id: id, content, ...(images.length > 0 && { images }) };
}

/** The change a regenerate of the reply after `parentId` asks for, or null for none. */
export function regenerateOf(parentId: string | null, current: GglibMessage[]): ChatChange | null {
  const at = parentId === null ? -1 : current.findIndex((m) => m.id === parentId);
  const id = savedRowId(current[at + 1]);
  return id === null ? null : { kind: 'regenerate', message_id: id };
}

/** The change Branch from here on `messageId` asks for, or null for a message not saved. */
export function branchOf(messageId: string, current: GglibMessage[]): ChatChange | null {
  const id = savedRowId(current.find((m) => m.id === messageId));
  return id === null ? null : { kind: 'branch', message_id: id };
}

/** What the open chat offers of its branches (ADR 0017). */
export interface Branching extends Omit<SavedView, 'messages'> {
  /** Answer the question the chat ends in. */
  retry: () => Promise<void>;
  /** Copy the chat as far as the end of the turn holding `messageId` into a new branch, and open it. */
  branchFrom: (messageId: string) => Promise<void>;
  /** Open chat `cid`, an option at a branch point. */
  open: (cid: number) => void;
}

/**
 * What the open chat offers of its branches, as the thread reads it: a new
 * object only when what the daemon says of the chat changes, so nothing
 * under it draws again for a render of the page. Each action is made by
 * the latest `change`, and opens through the latest `open`.
 */
export function useBranching(
  said: Omit<SavedView, 'messages'>,
  messages: RefObject<GglibMessage[]>,
  change: (made: ChatChange | null) => Promise<void>,
  open?: (cid: number) => void,
): Branching {
  const latest = useRef({ change, open });
  useEffect(() => {
    latest.current = { change, open };
  });
  const { answerable, points } = said;
  return useMemo(() => ({
    answerable,
    points,
    retry: () => latest.current.change(null),
    branchFrom: async (messageId: string) => {
      const made = branchOf(messageId, messages.current);
      if (made) await latest.current.change(made);
    },
    open: (cid: number) => latest.current.open?.(cid),
  }), [answerable, points, messages]);
}

/** What a change needs of the runtime it is made from. */
export interface ChangeDeps {
  conversationId: number | undefined;
  reader: {
    clearToSend: (cid: number) => Promise<boolean>;
    beginSend: () => AbortSignal | null;
    endReading: (signal: AbortSignal) => void;
    showSaved: (cid: number, signal: AbortSignal) => Promise<void>;
    follow: (cid: number, runId: string, signal: AbortSignal) => Promise<void>;
  };
  /** Start a run that answers the question `cid` ends in; its id. */
  answer: (cid: number) => Promise<string>;
  /** A chat changed, or a branch was made: the page lists it and opens it. */
  onConversationChanged?: (cid: number) => void;
  /**
   * A branch was made and opened; the chat the change was made on is as it
   * was. `unanswered` says why the branch's answer was not started, when it
   * was not: the branch then offers Retry.
   */
  onBranched?: (cid: number, unanswered?: Error) => void;
  onError?: (error: Error) => void;
}

/**
 * Make `change` to the open chat, or, with no change, answer the question
 * it ends in (Retry). Nothing is made while a reply is being read here, or
 * while opening has not learned whether one is. A refused change changes
 * nothing; a change made stands when its answer is refused, and the chat it
 * left is shown, offering Retry.
 */
export async function changeAndAnswer(change: ChatChange | null, deps: ChangeDeps): Promise<void> {
  const { conversationId: cid, reader } = deps;
  if (cid === undefined || !(await reader.clearToSend(cid))) return;
  const signal = reader.beginSend();
  if (!signal) return;
  let changed: ChatChanged | null = change ? null : { conversation_id: cid, forked: false, answer: true };
  let runId: string | null = null;
  let failed: Error | undefined;
  try {
    changed ??= await getTransport().changeConversation(cid, change!);
    if (changed.answer) runId = await deps.answer(changed.conversation_id);
  } catch (error) {
    failed = error as Error;
  }
  if (signal.aborted) return;
  const target = changed?.conversation_id ?? cid;
  if (changed?.forked) {
    reader.endReading(signal);
    deps.onConversationChanged?.(target);
    deps.onBranched?.(target, failed);
    return;
  }
  // What the change saved, or with nothing changed what was: shown even when it cannot be read again.
  await reader.showSaved(target, signal).catch(() => {});
  if (runId) await reader.follow(target, runId, signal);
  else reader.endReading(signal);
  if (failed) deps.onError?.(failed);
}
