/**
 * The chat's Thinking switch: whether the composer shows one, which way it
 * is, and what the next send says of it.
 *
 * Shown only where the chat's model thinks, by one rule (`thinks`): this
 * machine's model by its catalogue entry; the paired machine's by the row
 * that machine lists for it; a far chat's by the row that machine lists
 * under the name the chat last ran on (its settings' `model_name`, else its
 * last reply's). A model that is not known gets no switch, and so does every
 * model of a machine whose gglib predates the choice, which lists none as
 * thinking: the key it would refuse is never sent to it.
 *
 * The switch shows what gglib remembers of the chat (`settings.thinking`)
 * until it is clicked. From then it shows the choice made here, and a send
 * says that choice, `off` or `default`, for as long as it differs from what
 * gglib remembers. A choice ends in one of two ways. The turn that says it
 * is accepted: gglib remembers it from then, so no later send says it, and
 * it is shown only until the chat is next read, when that reading is shown
 * whatever it says. Or, before any turn that says it is accepted, a reading
 * of the chat agrees with it. A reading is this machine's list, which the
 * page reads after each run and as a chat is opened, or a far chat, read each
 * time it is opened and after each run. So gglib is told once, a send it
 * refused tells it again, and a chat changed on another device shows that
 * change at the next reading, also where the chat was left before its reply
 * ended and the reading after that reply was never made.
 *
 * Choices are held per chat and per machine, in memory, for as long as the
 * page is mounted: a far chat's id names another chat here.
 *
 * @module useThinkingSwitch
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import type { ChatSource, ConversationSummary } from '../services/transport';
import type { HubChatOpen } from '../types/generated/HubChatOpen';
import type { ModelRef } from '../types/generated/ModelRef';
import type { Thinking } from '../types/generated/Thinking';
import { turnMadeFromMetadata } from '../utils/messages/turnMade';
import { thinks } from '../utils/thinks';
import { usePairedModels } from './usePairedModels';

/** What a send says of the switch, and how the switch learns that its turn was accepted. */
export interface ThinkingSaid {
  /** The choice the turn carries. */
  said: Thinking;
  /** Called once the turn is accepted: its machine remembers the choice from then. Never for a refused one. */
  accepted: () => void;
}

/** The switch as the composer draws it, and as a send reads it. */
export interface ThinkingSwitch {
  /** Whether there is one: the open chat's model thinks. */
  shown: boolean;
  /** Which way it is: on, unless the chat is switched off. */
  on: boolean;
  /** Switch the open chat's the other way. */
  toggle: () => void;
  /**
   * What a send starting now says of it: `off` or `default` while the choice
   * made here is not yet what gglib remembers, and nothing otherwise.
   */
  forSend: () => ThinkingSaid | undefined;
}

/** What the switch reads about the open chat and its model. */
export interface ThinkingSwitchChat {
  /** The open chat's id on its machine; null with none open. */
  conversationId: number | null;
  /** This machine's chats as last listed, each with its settings. Not read for a far chat. */
  conversations: readonly Remembering[];
  /** A far chat: its settings and its model are in `farOpen`. */
  far: boolean;
  /** The far chat as its machine last answered it, when it is the one open. */
  farOpen: HubChatOpen | null;
  /** The paired machine's model the chat is with, if it is. */
  paired?: ModelRef;
  /** This machine's model: whether it thinks. */
  thinks: boolean;
}

/** A chat as gglib reported it: its id, and the settings that say what it remembers. */
type Remembering = Pick<ConversationSummary, 'id'> & { settings?: ConversationSummary['settings'] };

/** The choices made here that no accepted turn has said and no reading has agreed with: on (`true`) or off, by chat. */
type Choices = ReadonlyMap<string, boolean>;

/** What an accepted turn left a chat remembering, and the reading of the chat that was the newest then. */
interface Accepted {
  on: boolean;
  since: object;
}

const keyOf = (source: ChatSource, id: number) => `${source}:${id}`;

/** The far chat `id` as its machine last answered it; null when the answer held is another chat's. */
const farReading = (chat: ThinkingSwitchChat, id: number | null) =>
  chat.farOpen?.conversation.id === id ? chat.farOpen : null;

/**
 * The newest reading held of chat `id` on `source`: a far chat as its machine
 * last answered it, or the list the page holds, which while the far
 * machine's is shown is one no list of this machine's will ever be.
 */
const readingOf = (chat: ThinkingSwitchChat, source: ChatSource, id: number): object | null =>
  source === 'far' ? farReading(chat, id) : chat.conversations;

/** Whether `chat` thinks as gglib remembers it: yes, unless it remembers Off. */
const remembersOn = (chat: Remembering | null | undefined) => chat?.settings?.thinking !== 'off';

/** `choices` without those a reading of `chats` agrees with: gglib remembers them now. */
function settle(choices: Choices, source: ChatSource, chats: readonly Remembering[]): Choices {
  const agreed = chats.filter((chat) => choices.get(keyOf(source, chat.id)) === remembersOn(chat));
  if (agreed.length === 0) return choices;
  const left = new Map(choices);
  agreed.forEach((chat) => left.delete(keyOf(source, chat.id)));
  return left;
}

/** The name a far chat's model goes by: the one it last ran on, else its last reply's. */
function farModelName(open: HubChatOpen): string | undefined {
  const ranOn = open.conversation.settings?.model_name;
  if (ranOn) return ranOn;
  for (let i = open.messages.length - 1; i >= 0; i--) {
    const row = open.messages[i];
    const name = row.role === 'assistant' ? turnMadeFromMetadata(row.metadata)?.modelName : undefined;
    if (name) return name;
  }
  return undefined;
}

export function useThinkingSwitch(chat: ThinkingSwitchChat): ThinkingSwitch {
  const { conversationId, conversations, far, paired } = chat;
  const source: ChatSource = far ? 'far' : 'this';
  // A far chat read before this one was opened says nothing of this one.
  const farOpen = far ? farReading(chat, conversationId) : null;
  const { group } = usePairedModels(far || paired !== undefined);

  let modelThinks = chat.thinks;
  if (far) {
    const name = farOpen ? farModelName(farOpen) : undefined;
    const row = name === undefined ? undefined : group?.models.find((m) => m.id === name);
    modelThinks = row !== undefined && thinks(row);
  } else if (paired) {
    const row = group?.models.find((m) => m.gglib_id === paired.id);
    modelThinks = row !== undefined && thinks(row);
  }

  const [choices, setChoices] = useState<Choices>(() => new Map());
  const [accepted, setAccepted] = useState<ReadonlyMap<string, Accepted>>(() => new Map());
  // Each reading settles the choices it agrees with: this machine's list
  // every chat it names, a far chat its own. The far list says nothing of a
  // chat's settings, and settles none of this machine's.
  useEffect(() => {
    if (!far) setChoices((held) => settle(held, 'this', conversations));
  }, [far, conversations]);
  const farConversation = chat.farOpen?.conversation;
  useEffect(() => {
    if (farConversation) setChoices((held) => settle(held, 'far', [farConversation]));
  }, [farConversation]);

  const key = conversationId === null ? null : keyOf(source, conversationId);
  const reported = far ? farOpen?.conversation : conversations.find((c) => c.id === conversationId);
  // What gglib remembers of the open chat: what a turn accepted since its
  // newest reading said, and else that reading.
  const sent = key === null ? undefined : accepted.get(key);
  const reading = conversationId === null ? null : readingOf(chat, source, conversationId);
  const remembered = sent !== undefined && sent.since === reading ? sent.on : remembersOn(reported);
  const choice = key === null ? undefined : choices.get(key);
  const shown = key !== null && modelThinks;
  const on = choice ?? remembered;
  const said: Thinking | undefined =
    shown && choice !== undefined && choice !== remembered ? (choice ? 'default' : 'off') : undefined;

  // Read when a send starts or is accepted, which can be well after the
  // render that made it, and with another chat open.
  const latest = useRef<{ chat: ThinkingSwitchChat; sending?: ThinkingSaid }>({ chat });
  /**
   * A turn that said `on` of chat `id` was accepted. The choice it carried is
   * over, where it is still the one held; what it left the chat remembering
   * is kept until the chat is next read, unless no reading of it is held.
   */
  const accept = useCallback((of: ChatSource, id: number, on: boolean) => {
    const since = readingOf(latest.current.chat, of, id);
    setChoices((held) => {
      if (held.get(keyOf(of, id)) !== on) return held;
      const left = new Map(held);
      left.delete(keyOf(of, id));
      return left;
    });
    if (since) setAccepted((held) => new Map(held).set(keyOf(of, id), { on, since }));
  }, []);
  const sending: ThinkingSaid | undefined =
    said === undefined || conversationId === null || choice === undefined
      ? undefined
      : { said, accepted: () => accept(source, conversationId, choice) };
  useEffect(() => {
    latest.current = { chat, sending };
  });
  const forSend = useCallback(() => latest.current.sending, []);
  const toggle = useCallback(() => {
    if (key === null) return;
    setChoices((held) => new Map(held).set(key, !(held.get(key) ?? remembered)));
  }, [key, remembered]);

  return { shown, on, toggle, forSend };
}
