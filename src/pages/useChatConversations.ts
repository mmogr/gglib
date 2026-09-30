/**
 * The chat page's conversation list, from one source at a time: this
 * machine's, or the far machine's through this machine's daemon.
 *
 * This machine's list gets a first conversation when it has none; the far
 * machine's never does, because a chat is made on the machine that holds
 * it. A list that arrives after the source changed is dropped.
 *
 * @module useChatConversations
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import type { FetchedList } from '../components/ConversationListPanel';
import { DEFAULT_SYSTEM_PROMPT } from '../hooks/useGglibRuntime';
import { getTransport } from '../services/transport';
import type { ChatSource, ConversationSummary } from '../services/transport';
import type { HubChat } from '../types/generated/HubChat';

const DEFAULT_CONVERSATION_TITLE = 'New Chat';

/** A far chat as the list shows one. Its prompt is read when it is opened. */
function summaryOf(chat: HubChat): ConversationSummary {
  return {
    id: chat.id,
    title: chat.title,
    model_id: chat.model_id ?? null,
    system_prompt: null,
    settings: null,
    created_at: chat.updated_at,
    updated_at: chat.updated_at,
  };
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

export function useChatConversations(initialId: number | null, onError: (message: string) => void) {
  const [source, setSource] = useState<ChatSource>('this');
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
        let list: ConversationSummary[];
        let preferredId = options.preferredId ?? null;
        if (asked === 'far') list = (await getTransport().listFarChats()).map(summaryOf);
        else [list, preferredId] = await listHere(preferredId);
        if (sourceRef.current !== asked) return;
        setConversations(list);
        setFetched({ ids: list.map((c) => c.id), askedAt, source: asked });
        setActiveConversationId((prev) => {
          if (preferredId && list.some((c) => c.id === preferredId)) return preferredId;
          if (prev && list.some((c) => c.id === prev)) return prev;
          return list[0]?.id ?? null;
        });
      } catch (error) {
        if (sourceRef.current === asked) errorRef.current(error instanceof Error ? error.message : String(error));
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
    activeConversationIdRef,
    fetched,
    syncConversations,
  };
}
