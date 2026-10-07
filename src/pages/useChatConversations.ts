/**
 * The chat page's conversation list, from one source at a time: this
 * machine's, or the far machine's through this machine's daemon.
 *
 * This machine's list gets a first conversation when it has none; the far
 * machine's never does, because a chat is made on the machine that holds
 * it. A list that arrives after the source changed is dropped.
 *
 * A conversation's machine is fixed, so this machine's list holds only the
 * conversations the session's model can carry on: those that ran on its
 * machine, and those that have not run yet.
 *
 * @module useChatConversations
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import type { FetchedList } from '../components/ConversationListPanel';
import { DEFAULT_SYSTEM_PROMPT } from '../hooks/useGglibRuntime';
import { getTransport } from '../services/transport';
import type { ChatSource, ConversationSummary } from '../services/transport';
import type { HubChat } from '../types/generated/HubChat';
import type { Machine } from '../types/generated/Machine';
import { formatError } from '../utils/errors';

const DEFAULT_CONVERSATION_TITLE = 'New Chat';

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
  };
}

/**
 * The machine a conversation ran on: its stored model's, or this one when
 * it stores only a `model_id`; `null` for one that has not run yet.
 */
function machineOf(conversation: ConversationSummary): Machine | null {
  if (conversation.settings?.model) return conversation.settings.model.machine;
  return conversation.model_id === null ? null : { kind: 'local' };
}

/** Whether a session on `machine` may carry `conversation` on. */
function carriesOn(machine: Machine, conversation: ConversationSummary): boolean {
  const ran = machineOf(conversation);
  if (ran === null || machine.kind === 'local') return ran === null || ran.kind === 'local';
  return ran.kind === 'paired' && ran.fingerprint === machine.fingerprint;
}

/** This machine's list, with a first conversation made if it has none. */
async function listHere(preferredId: number | null): Promise<[ConversationSummary[], number | null]> {
  let list = await getTransport().listConversations();
  if (list.length) return [list, preferredId];
  const created = await getTransport().createConversation({
    title: DEFAULT_CONVERSATION_TITLE,
    modelId: null,
    systemPrompt: DEFAULT_SYSTEM_PROMPT,
  });
  list = await getTransport().listConversations();
  return [list, created];
}

export function useChatConversations(
  initialId: number | null,
  onError: (message: string) => void,
  machine: Machine = { kind: 'local' },
) {
  const [source, setSource] = useState<ChatSource>('this');
  // The session's machine, read by a list when it lands.
  const machineRef = useRef(machine);
  useEffect(() => { machineRef.current = machine; });
  const sourceRef = useRef(source);
  const [conversations, setConversations] = useState<ConversationSummary[]>([]);
  const [conversationLoading, setConversationLoading] = useState(true);
  const [activeConversationId, setActiveConversationId] = useState<number | null>(initialId);
  // Read by a model switch when it lands, which can be well after the pick.
  const activeConversationIdRef = useRef(activeConversationId);
  useEffect(() => { activeConversationIdRef.current = activeConversationId; }, [activeConversationId]);
  // The list as the daemon last sent it: the only word a New mark is dropped on.
  const [fetched, setFetched] = useState<FetchedList | null>(null);
  const errorRef = useRef(onError);
  useEffect(() => { errorRef.current = onError; });

  const syncConversations = useCallback(
    async (options: { preferredId?: number | null; silent?: boolean } = {}) => {
      const asked = sourceRef.current;
      if (!options.silent) setConversationLoading(true);
      try {
        const askedAt = Date.now();
        let all: ConversationSummary[];
        let preferredId = options.preferredId ?? null;
        if (asked === 'far') all = (await getTransport().listFarChats()).map(summaryOf);
        else [all, preferredId] = await listHere(preferredId);
        if (sourceRef.current !== asked) return;
        // A New mark on a chat of the other machine is kept: it is listed, only not here.
        const list = asked === 'far' ? all : all.filter((c) => carriesOn(machineRef.current, c));
        setConversations(list);
        setFetched({ ids: all.map((c) => c.id), askedAt, source: asked });
        setActiveConversationId((prev) => {
          if (preferredId && list.some((c) => c.id === preferredId)) return preferredId;
          if (prev && list.some((c) => c.id === prev)) return prev;
          return list[0]?.id ?? null;
        });
      } catch (error) {
        if (sourceRef.current === asked) errorRef.current(formatError(error));
      } finally {
        if (!options.silent && sourceRef.current === asked) setConversationLoading(false);
      }
    },
    [],
  );

  /** Show `next`'s chats instead, from the top of its list. */
  const switchSource = useCallback((next: ChatSource) => {
    if (next === sourceRef.current) return;
    sourceRef.current = next;
    setSource(next);
    setConversations([]);
    setActiveConversationId(null);
    void syncConversations();
  }, [syncConversations]);

  /**
   * The conversation a model switch opens when it lands: this machine's
   * open one, or none while a far chat is open, since its id means another
   * chat here. Read when the switch lands, not when it was asked for.
   */
  const landingConversationId = useCallback(
    (): number | null => (sourceRef.current === 'far' ? null : activeConversationIdRef.current),
    [],
  );

  // Load conversations on mount
  useEffect(() => {
    void syncConversations();
  }, [syncConversations]);

  return {
    source,
    switchSource,
    conversations,
    setConversations,
    conversationLoading,
    activeConversationId,
    setActiveConversationId,
    landingConversationId,
    fetched,
    syncConversations,
  };
}
