/**
 * The chat's tools: which of the tools here its runs may call, as the
 * composer's Tools ▸ ticks them, saved with the chat.
 *
 * gglib reads them from the chat's settings on each of its turns: `tools`
 * names the tools a run may call (each by the name the daemon knows it by,
 * `<server id>:<tool>` or `builtin:<tool>`, or by a bare name, which any
 * server's tool of that name answers), none named is every tool of every
 * enabled server, and `no_tools` is none at all. A new chat saves none, so
 * its runs may call every tool. A tick or an untick writes the change at
 * once (`PUT /api/conversations/{id}`), with only the setting it changes;
 * the list never becomes empty by an untick, since an empty list is every
 * tool, so the last untick saves `no_tools` instead.
 *
 * What is shown is the choice made here until the write lands and the chat
 * is read again, then the reading. A write gglib refused shows the reading,
 * and `onError` says why. A chat not yet made holds its choice in memory
 * until its first send makes it: that send writes it before its turn starts
 * (`forCreate`). None on a far chat: its tools are its machine's.
 *
 * @module useChatTools
 */

import { useCallback, useRef, useState } from 'react';
import { refreshChats } from '../services/chatRegistry';
import { getTransport, type ConversationSummary } from '../services/transport';
import type { ConversationSettingsPatch } from '../types/generated/ConversationSettingsPatch';
import { formatError } from '../utils/errors';

/** The chat's tools as gglib reads them: the tools it names (none is every one), and whether they are off. */
export interface SavedTools {
  tools: readonly string[];
  off: boolean;
}

/** A chat that saved nothing: every tool. */
export const EVERY_TOOL: SavedTools = { tools: [], off: false };

/** What a first send writes of the choice made before the chat was made. */
export interface ToolsSaid {
  patch: ConversationSettingsPatch;
  /** The patch was written to the chat the send made. */
  accepted: () => void;
}

/** The checklist's state and its two actions. */
export interface ChatTools {
  /** Whether the tool the daemon knows by `name` is ticked. */
  ticked: (name: string) => boolean;
  /** Every tool is ticked, as a chat that saved nothing is. */
  every: boolean;
  /** No tool is. */
  none: boolean;
  /** Tick or untick one tool; `listed` is every tool the checklist shows, by the daemon's names. */
  toggle: (name: string, on: boolean, listed: readonly string[]) => void;
  /** Tick every tool, or none. */
  all: (on: boolean) => void;
  /** What a first send writes before its turn starts, for a choice made before the chat was made. */
  forCreate: () => ToolsSaid | undefined;
}

/** The chat's tools as its settings save them. */
export function savedOf(settings: ConversationSummary['settings']): SavedTools {
  return { tools: settings?.tools ?? [], off: settings?.no_tools === true };
}

/**
 * Whether the saved entry `entry` names the tool `name`, as gglib reads the
 * list: exactly, or by the bare name when the entry has no `:`.
 */
function names(entry: string, name: string): boolean {
  if (entry === name) return true;
  const colon = name.indexOf(':');
  return !entry.includes(':') && colon >= 0 && entry === name.slice(colon + 1);
}

/** Whether `saved` lets a run call the tool `name`. */
export function isTicked(saved: SavedTools, name: string): boolean {
  if (saved.off) return false;
  return saved.tools.length === 0 || saved.tools.some((entry) => names(entry, name));
}

/**
 * The chat's tools once `name` is ticked (`on`) or not, of the tools
 * `listed`. From every tool, an untick names every listed tool but that
 * one. An untick that would leave the list empty turns the tools off, the
 * list kept as it was, since an empty list is every tool.
 */
export function afterToggle(saved: SavedTools, name: string, on: boolean, listed: readonly string[]): SavedTools {
  const named = saved.off ? [] : saved.tools.length === 0 ? [...listed] : [...saved.tools];
  const kept = on
    ? named.some((entry) => names(entry, name)) ? named : [...named, name]
    : named.filter((entry) => !names(entry, name));
  return kept.length === 0 ? { tools: saved.tools, off: true } : { tools: kept, off: false };
}

/** The chat's tools once every tool is ticked (`on`), or none: none keeps the list it names. */
export function afterAll(saved: SavedTools, on: boolean): SavedTools {
  return on ? EVERY_TOOL : { tools: saved.tools, off: true };
}

/** The settings a change from `before` to `after` writes: only those it changes. */
export function patchOf(before: SavedTools, after: SavedTools): ConversationSettingsPatch {
  const patch: ConversationSettingsPatch = {};
  const same = before.tools.length === after.tools.length && before.tools.every((entry, i) => entry === after.tools[i]);
  if (!same) patch.tools = [...after.tools];
  if (before.off !== after.off) patch.no_tools = after.off;
  return patch;
}

const isEmpty = (patch: ConversationSettingsPatch) => Object.keys(patch).length === 0;

export interface ChatToolsChat {
  /** The open chat's id; null for one not yet made. */
  conversationId: number | null;
  /** This machine's chats as last listed, each with its settings. */
  conversations: readonly ConversationSummary[];
  /** A far chat: its tools are its machine's, and none are offered here. */
  far: boolean;
  /** A write gglib refused, said in a sentence. */
  onError: (sentence: string) => void;
}

/** The key a choice is held under: the chat's id, or the chat not yet made. */
const keyOf = (id: number | null) => (id === null ? 'new' : String(id));

export function useChatTools({ conversationId, conversations, far, onError }: ChatToolsChat): ChatTools | undefined {
  const [held, setHeld] = useState<ReadonlyMap<string, SavedTools>>(new Map());
  // Writes go one after another, so the last choice made is the one saved.
  const writes = useRef<Promise<void>>(Promise.resolve());
  const key = keyOf(conversationId);
  const reading = conversationId === null ? EVERY_TOOL : savedOf(conversations.find((c) => c.id === conversationId)?.settings);
  const shown = held.get(key) ?? reading;

  const drop = useCallback((at: string, choice: SavedTools) => {
    setHeld((now) => {
      if (now.get(at) !== choice) return now;
      const left = new Map(now);
      left.delete(at);
      return left;
    });
  }, []);

  const choose = (next: SavedTools) => {
    const patch = patchOf(shown, next);
    if (isEmpty(patch)) return;
    setHeld((now) => new Map(now).set(key, next));
    if (conversationId === null) return;
    const id = conversationId;
    writes.current = writes.current.then(async () => {
      try {
        await getTransport().updateConversationSettings(id, patch);
        await refreshChats('this');
      } catch (error) {
        onError(`Could not save this chat's tools: ${formatError(error)}`);
      } finally {
        drop(key, next);
      }
    });
  };

  if (far) return undefined;
  return {
    ticked: (name) => isTicked(shown, name),
    every: !shown.off && shown.tools.length === 0,
    none: shown.off,
    toggle: (name, on, listed) => choose(afterToggle(shown, name, on, listed)),
    all: (on) => choose(afterAll(shown, on)),
    forCreate: () => {
      const choice = held.get('new');
      if (conversationId !== null || choice === undefined) return undefined;
      const patch = patchOf(EVERY_TOOL, choice);
      return isEmpty(patch) ? undefined : { patch, accepted: () => drop('new', choice) };
    },
  };
}
