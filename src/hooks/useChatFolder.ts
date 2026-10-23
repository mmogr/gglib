/**
 * The chat's folder: where gglib's own file tools read in its runs on this
 * machine, which folder the composer shows, and what the next send says of
 * it.
 *
 * The folder shown is the choice made here, else the one gglib remembers of
 * the chat (`settings.folder`, the canonical path the daemon recorded). A
 * choice is a folder picked with the native dialog, or on web a path typed,
 * which the daemon checks; Clear is a choice too, of no folder. A send says
 * the choice, the path or `""` for Clear, for as long as it differs from
 * what gglib remembers. It ends as a Thinking choice does: the turn that
 * says it is accepted, and gglib remembers it from then, so it is shown
 * only until the chat is next read and that reading is shown whatever it
 * says; or a reading of the chat agrees with it first. Until that reading,
 * an accepted path is shown as it was said; the reading shows the canonical
 * path the daemon recorded, which is the one read.
 *
 * A send the daemon refused for its folder leaves the chat in an error
 * state: `folder_unusable` for a path the send said, `folder_missing` for
 * the remembered one, gone since. Choosing another folder or clearing it
 * ends that state, and so does a turn that is accepted.
 *
 * There is none on a far chat: a paired device's turn never reads a folder,
 * and the far machine's page is where its chats' folders are chosen. A chat
 * with this machine's or the paired machine's model has one, since the loop
 * and its tools run here; with the paired machine's model, what the tools
 * read is sent to that machine (`sentTo`). A model that calls no tools
 * shows none, unless its chat has a folder already, which can then be seen,
 * as not read (`unread`), and cleared.
 *
 * Choices are held per chat, in memory, for as long as the page is mounted;
 * a chat not yet made is held as the new one, and its choice moves to the
 * chat its first send makes.
 *
 * @module useChatFolder
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import { isDesktop, pickFolder } from '../services/platform';
import type { ConversationSummary } from '../services/transport';
import { codeOf } from './useGglibRuntime/imageRefusals';

/** The error state of a chat whose remembered folder is gone. */
export const FOLDER_GONE = 'This folder is gone. Choose another or clear it.';

/** What a send says of the folder, and how the chat learns what became of it. */
export interface FolderSaid {
  /** The folder the turn says: a path sets it, `""` forgets it. Absent where the turn runs as the chat remembers. */
  said?: string;
  /**
   * The turn was accepted, in chat `conversationId` (the one it made, for a
   * chat not yet made): gglib remembers what it said from then.
   */
  accepted: (conversationId?: number) => void;
  /**
   * The turn was refused, in chat `conversationId` where one was made: the
   * sentence a person reads when it was refused for its folder, else null.
   */
  refused: (error: unknown, conversationId?: number) => string | null;
}

/** The folder as the composer draws it, and as a send reads it. */
export interface ChatFolder {
  /** Whether the composer offers one. */
  shown: boolean;
  /** The folder as shown: the choice made here, else the one remembered; null for none. */
  path: string | null;
  /** Why the chat's last send was refused for its folder; null when it was not. */
  error: string | null;
  /** Whether Change opens the native dialog; on web the path is typed. */
  native: boolean;
  /** The paired machine whose model this chat's turns, and what its file tools read, are sent to; null for this machine's model. */
  sentTo: string | null;
  /** Whether the model calls no tools, so the folder shown is not read. */
  unread: boolean;
  /** Open the native dialog and choose the folder it answers; nothing when it is cancelled. */
  pick: () => Promise<void>;
  /** Choose the folder at `path`, as typed. */
  choose: (path: string) => void;
  /** Choose no folder: the next send forgets the one remembered. */
  clear: () => void;
  /** What a send starting now says of it; nothing where none is shown. */
  forSend: () => FolderSaid | undefined;
}

/** What the folder reads about the open chat and its model. */
export interface ChatFolderChat {
  /** The open chat's id; null for one not yet made. */
  conversationId: number | null;
  /** This machine's chats as last listed, each with its settings. */
  conversations: readonly Remembering[];
  /** A far chat, which has no folder here. */
  far: boolean;
  /** Whether this machine's model calls tools; null or absent when unknown. */
  supportsToolCalls?: boolean | null;
  /** The name of the paired machine whose model the chat runs on; absent for this machine's model. */
  sentTo?: string | null;
}

/** A chat as gglib reported it: its id, and the settings that say what it remembers. */
type Remembering = Pick<ConversationSummary, 'id'> & { settings?: ConversationSummary['settings'] };

/** The choices no accepted turn has said and no reading has agreed with, by chat: a path, or `""` for none. */
type Choices = ReadonlyMap<string, string>;

/** What an accepted turn left a chat remembering, and the list that was the newest then. */
interface Accepted {
  folder: string | null;
  since: object;
}

const keyOf = (id: number | null) => (id === null ? 'new' : String(id));
const folderOf = (choice: string) => (choice === '' ? null : choice);
const rememberedOf = (chat: Remembering | undefined) => chat?.settings?.folder ?? null;

/** `held` without `key`, or `held` itself when it has none. */
function without<V>(held: ReadonlyMap<string, V>, ...keys: string[]): ReadonlyMap<string, V> {
  if (!keys.some((key) => held.has(key))) return held;
  const left = new Map(held);
  keys.forEach((key) => left.delete(key));
  return left;
}

/** `choices` without those a reading of `chats` agrees with: gglib remembers them now. */
function settle(choices: Choices, chats: readonly Remembering[]): Choices {
  const agreed = chats.filter((chat) => {
    const choice = choices.get(keyOf(chat.id));
    return choice !== undefined && folderOf(choice) === rememberedOf(chat);
  });
  return without(choices, ...agreed.map((chat) => keyOf(chat.id)));
}

/** The sentence for a refusal of a run for its folder, by the daemon's code; null for any other. */
export function folderRefusal(error: unknown): string | null {
  const code = codeOf(error);
  if (code === 'folder_missing') return FOLDER_GONE;
  if (code !== 'folder_unusable') return null;
  const told = error instanceof Error && error.message ? `: ${error.message}` : '';
  return `This folder cannot be read${told}. Choose another or clear it.`;
}

export function useChatFolder(chat: ChatFolderChat): ChatFolder {
  const { conversationId, conversations, far, supportsToolCalls, sentTo = null } = chat;
  const [choices, setChoices] = useState<Choices>(() => new Map());
  const [accepted, setAccepted] = useState<ReadonlyMap<string, Accepted>>(() => new Map());
  const [errors, setErrors] = useState<ReadonlyMap<string, string>>(() => new Map());
  useEffect(() => {
    if (!far) setChoices((held) => settle(held, conversations));
  }, [far, conversations]);

  const key = far ? null : keyOf(conversationId);
  const reported = conversationId === null ? undefined : conversations.find((c) => c.id === conversationId);
  // What gglib remembers of the open chat: what a turn accepted since the
  // newest list said, and else that list.
  const sent = key === null ? undefined : accepted.get(key);
  const remembered = sent !== undefined && sent.since === conversations ? sent.folder : rememberedOf(reported);
  const choice = key === null ? undefined : choices.get(key);
  const path = key === null ? null : choice !== undefined ? folderOf(choice) : remembered;
  const error = key === null ? null : (errors.get(key) ?? null);
  const shown = key !== null && (supportsToolCalls !== false || path !== null);
  const said = choice !== undefined && folderOf(choice) !== remembered ? choice : undefined;

  // Read when a send starts, is accepted or is refused, which can be well
  // after the render that made it, and with another chat open.
  const latest = useRef<{ conversations: readonly Remembering[]; sending?: FolderSaid }>({ conversations });
  /** A turn from chat `from`, which saved to `to`, was accepted: what it said is remembered. */
  const accept = useCallback((from: string, to: string, told: string | undefined) => {
    if (told !== undefined) {
      setChoices((held) => (held.get(from) === told ? without(held, from) : held));
      setAccepted((held) => new Map(held).set(to, { folder: folderOf(told), since: latest.current.conversations }));
    }
    setErrors((held) => without(held, from, to));
  }, []);
  /** A turn from chat `from`, which made `to`, was refused: a choice made before `to` was is its now. */
  const refuse = useCallback((from: string, to: string, error: unknown) => {
    const sentence = folderRefusal(error);
    if (from !== to) {
      setChoices((held) => {
        const moved = held.get(from);
        return moved === undefined ? held : new Map(without(held, from)).set(to, moved);
      });
    }
    if (sentence) setErrors((held) => new Map(held).set(to, sentence));
    return sentence;
  }, []);
  const sending: FolderSaid | undefined =
    !shown || key === null
      ? undefined
      : {
          ...(said !== undefined && { said }),
          accepted: (cid) => accept(key, cid === undefined ? key : keyOf(cid), said),
          refused: (failure, cid) => refuse(key, cid === undefined ? key : keyOf(cid), failure),
        };
  useEffect(() => {
    latest.current = { conversations, sending };
  });
  const forSend = useCallback(() => latest.current.sending, []);

  const choose = useCallback(
    (typed: string) => {
      const next = typed.trim();
      if (key === null || next === '') return;
      setChoices((held) => new Map(held).set(key, next));
      setErrors((held) => without(held, key));
    },
    [key],
  );
  const clear = useCallback(() => {
    if (key === null) return;
    setChoices((held) => new Map(held).set(key, ''));
    setErrors((held) => without(held, key));
  }, [key]);
  const pick = useCallback(async () => {
    try {
      const picked = await pickFolder();
      if (picked !== null) choose(picked);
    } catch (failure) {
      const told = failure instanceof Error ? failure.message : String(failure);
      if (key !== null) setErrors((held) => new Map(held).set(key, `The folder dialog did not open: ${told}`));
    }
  }, [choose, key]);

  const unread = supportsToolCalls === false;
  return { shown, path, error, native: isDesktop(), sentTo, unread, pick, choose, clear, forSend };
}
