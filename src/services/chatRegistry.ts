/**
 * The chats, as the sidebar lists them and the open chat reads its own:
 * this machine's, and those of the machine it is joined to (read through
 * this machine's daemon, never copied).
 *
 * One store, so a chat renamed, deleted, started or titled in the place is
 * in the sidebar's list at once. A list is read again when something asks
 * (`refreshChats`): the sidebar when it mounts and when a reply ends, the
 * place after anything it changes. A read that lands after a newer one was
 * asked is dropped; one that fails keeps the list it had.
 *
 * @module services/chatRegistry
 */

import { createEventStore } from './createEventStore';
import { getTransport } from './transport';
import type { ChatSource, ConversationSummary } from './transport';
import type { HubChat } from '../types/generated/HubChat';
import type { ModelRef } from '../types/generated/ModelRef';

/**
 * A conversation list as the daemon sent it, and when it was asked for.
 * Never a list built or patched on this side: the only word a New mark is
 * dropped on.
 */
export interface FetchedList {
  ids: readonly number[];
  /** When the fetch was sent, in ms since the epoch. */
  askedAt: number;
  /** Whose list it is; this machine's when absent. */
  source?: ChatSource;
}

interface SourceChats {
  /** The list, newest first; `null` until the first read lands. */
  list: ConversationSummary[] | null;
  fetched: FetchedList | null;
}

type ChatsState = Record<ChatSource, SourceChats>;

const EMPTY: ChatsState = {
  this: { list: null, fetched: null },
  far: { list: null, fetched: null },
};

const store = createEventStore<ChatsState>(EMPTY);
const asked: Record<ChatSource, number> = { this: 0, far: 0 };

/**
 * A far chat as the list shows one. Its prompt is read when it is opened.
 * Its `model_id` is the far machine's, which names another model here, so
 * the summary, a conversation of this machine's shape, carries none.
 */
function summaryOf(chat: HubChat): ConversationSummary {
  return {
    id: chat.id,
    title: chat.title,
    model_id: null,
    system_prompt: null,
    created_at: chat.updated_at,
    updated_at: chat.updated_at,
    ...(chat.branch_of !== undefined && { branch_of: chat.branch_of }),
  };
}

const newestFirst = (a: ConversationSummary, b: ConversationSummary) => b.updated_at.localeCompare(a.updated_at);

/** Read `source`'s list again. Resolves once it has landed (or failed). */
export async function refreshChats(source: ChatSource = 'this'): Promise<void> {
  const ticket = ++asked[source];
  const askedAt = Date.now();
  let list: ConversationSummary[];
  try {
    list =
      source === 'far'
        ? (await getTransport().listFarChats()).map(summaryOf)
        : await getTransport().listConversations();
  } catch {
    // The list it had stays; an empty first read says there is nothing.
    if (ticket === asked[source] && store.getState()[source].list === null) {
      store.setState({ ...store.getState(), [source]: { list: [], fetched: null } });
    }
    return;
  }
  if (ticket !== asked[source]) return;
  const sorted = [...list].sort(newestFirst);
  store.setState({
    ...store.getState(),
    [source]: { list: sorted, fetched: { ids: list.map((c) => c.id), askedAt, source } },
  });
}

/** `source`'s chats, newest first, and the list as the daemon last sent it. */
export function useChats(source: ChatSource = 'this'): SourceChats {
  return store.useStore()[source];
}

export function getChats(source: ChatSource = 'this'): SourceChats {
  return store.getState()[source];
}

/** Forget both lists, for a test, or a far machine that is gone. */
export function resetChats(source?: ChatSource): void {
  if (source === undefined) {
    asked.this += 1;
    asked.far += 1;
    store.setState(EMPTY);
    return;
  }
  asked[source] += 1;
  store.setState({ ...store.getState(), [source]: EMPTY[source] });
}

/**
 * The model a conversation of this machine's runs on: one of this machine's
 * (`local`, by its id here), one of the paired machine's (`paired`), or
 * `null` for one that has not run yet and names none.
 */
export type ChatModel = { kind: 'local'; id: number } | { kind: 'paired'; model: ModelRef } | null;

export function modelOf(conversation: ConversationSummary): ChatModel {
  const stored = conversation.settings?.model;
  if (stored?.machine.kind === 'paired') return { kind: 'paired', model: stored };
  if (stored) return { kind: 'local', id: stored.id };
  return conversation.model_id === null ? null : { kind: 'local', id: conversation.model_id };
}

/** The newest of `list` that runs on this machine's model `modelId`. */
export function latestOn(list: readonly ConversationSummary[], modelId: number): ConversationSummary | null {
  return list.find((c) => {
    const model = modelOf(c);
    return model?.kind === 'local' && model.id === modelId;
  }) ?? null;
}

/** The newest of `list` that runs on the paired machine's model `far`. */
export function latestOnFar(list: readonly ConversationSummary[], far: ModelRef): ConversationSummary | null {
  return list.find((c) => {
    const model = modelOf(c);
    return (
      model?.kind === 'paired' &&
      model.model.id === far.id &&
      far.machine.kind === 'paired' &&
      model.model.machine.kind === 'paired' &&
      model.model.machine.fingerprint === far.machine.fingerprint
    );
  }) ?? null;
}

/** The newest conversation that ran on one of this machine's models: the last one used. */
export function lastUsedModel(list: readonly ConversationSummary[]): number | null {
  for (const c of list) {
    const model = modelOf(c);
    if (model?.kind === 'local') return model.id;
  }
  return null;
}

/** This machine's list, read first if it has not been. */
export async function chatsHere(): Promise<ConversationSummary[]> {
  if (store.getState().this.list === null) await refreshChats('this');
  return store.getState().this.list ?? [];
}

/** Delete one of this machine's conversations, and read the list again. */
export async function deleteChat(id: number): Promise<void> {
  await getTransport().deleteConversation(id);
  await refreshChats('this');
}

/** Rename one of this machine's conversations, and read the list again. */
export async function renameChat(id: number, title: string): Promise<void> {
  await getTransport().updateConversationTitle(id, title);
  await refreshChats('this');
}
