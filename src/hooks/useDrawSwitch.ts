/**
 * The composer's Draw button: whether a message in the open chat can draw,
 * why not, whether the next one is to, and what its send says.
 *
 * Whether it can is the answer of the machine that would draw
 * (`drawingAvailability`): this machine's for its own chats, told what the
 * page knows of the chat's model (that it is the paired machine's, that it
 * calls no tools), and the far machine's for a far chat. Until an answer
 * comes, and when none does, the button is greyed and says so; an answer of
 * no greys it with that machine's reason. It is asked as a chat is opened,
 * when what is known of the model changes, and when `refresh` does.
 *
 * Pressed, the button is armed for the open chat alone, and the next send
 * says `draw`. It stays armed until the turn that says it is accepted, so a
 * refused send says it again, and every message after that one is sent
 * without it until the button is pressed again. A button that is greyed is
 * never armed, and its send says nothing: a machine is sent `draw` only
 * after it answered that it can.
 *
 * Held per chat and per machine, in memory, for as long as the page is
 * mounted; a chat not yet made is held as the new one.
 *
 * @module useDrawSwitch
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import { getTransport, type ChatSource } from '../services/transport';
import type { DrawingAvailability } from '../types/generated/DrawingAvailability';
import type { ModelRef } from '../types/generated/ModelRef';

/** What a send says of the button, and how the button learns that its turn was accepted. */
export interface DrawSaid {
  /** Called once the turn is accepted: the button is no longer armed. Never for a refused one. */
  accepted: () => void;
}

/** The button as the composer draws it, and as a send reads it. */
export interface DrawSwitch {
  /** Whether a message here can draw; greyed when not. */
  available: boolean;
  /** Why it cannot, in the words of the machine that would draw; absent when it can. */
  reason?: string;
  /** The image model that would draw, when it can and the machine names it. */
  model?: string;
  /** Whether the next send says `draw`. Never while greyed. */
  armed: boolean;
  /** Arm the open chat's next send, or disarm it. Nothing while greyed. */
  toggle: () => void;
  /** What a send starting now says of it: something only while armed. */
  forSend: () => DrawSaid | undefined;
}

/** What the button reads about the open chat and its model. */
export interface DrawSwitchChat {
  /** The open chat's id on its machine; null for one not yet made. */
  conversationId: number | null;
  /** A far chat: the far machine answers whether it can draw. */
  far: boolean;
  /** The paired machine's model the chat is with, if it is. */
  paired?: ModelRef;
  /** Whether this machine's model calls tools; null or absent when unknown. */
  supportsToolCalls?: boolean | null;
  /** Anything whose change may change the answer, such as the default image model: asked again when it does. */
  refresh?: unknown;
}

/** Shown while no answer has come. */
export const DRAW_CHECKING = 'Checking whether a message here can draw.';
/** Shown, before the failure's own words, when the question could not be asked. */
export const DRAW_UNCHECKED = 'Whether a message here can draw could not be checked';
/** Shown when a machine answers no and gives no reason. */
export const DRAW_UNAVAILABLE = 'A message here cannot draw.';

/** An answer, and the question it answers. */
interface Answer {
  question: string;
  availability: DrawingAvailability;
}

const keyOf = (source: ChatSource, id: number | null) => `${source}:${id ?? 'new'}`;

export function useDrawSwitch(chat: DrawSwitchChat): DrawSwitch {
  const { conversationId, far, refresh } = chat;
  const source: ChatSource = far ? 'far' : 'this';
  const pairedModel = !far && chat.paired !== undefined;
  const callsTools = far ? null : (chat.supportsToolCalls ?? null);
  // What is asked: an answer to another question says nothing of this one.
  const question = `${source}:${pairedModel}:${callsTools}`;

  const [answer, setAnswer] = useState<Answer | null>(null);
  useEffect(() => {
    let left = false;
    const answered = (availability: DrawingAvailability) => {
      if (!left) setAnswer({ question, availability });
    };
    getTransport()
      .drawingAvailability(source, { far: pairedModel, callsTools })
      .then(answered, (error: unknown) =>
        answered({ available: false, reason: `${DRAW_UNCHECKED}: ${error instanceof Error ? error.message : String(error)}` }),
      );
    return () => {
      left = true;
    };
  }, [question, source, pairedModel, callsTools, conversationId, refresh]);

  const availability = answer?.question === question ? answer.availability : null;
  const available = availability?.available === true;
  const reason = available ? undefined : availability ? (availability.reason ?? DRAW_UNAVAILABLE) : DRAW_CHECKING;

  /** The chats whose next send is to draw. */
  const [pressed, setPressed] = useState<ReadonlySet<string>>(() => new Set());
  const key = keyOf(source, conversationId);
  const armed = available && pressed.has(key);

  const disarm = useCallback((of: string) => {
    setPressed((held) => {
      if (!held.has(of)) return held;
      const left = new Set(held);
      left.delete(of);
      return left;
    });
  }, []);
  // Read when a send starts, which can be well after the render that made it.
  const sending: DrawSaid | undefined = armed ? { accepted: () => disarm(key) } : undefined;
  const latest = useRef(sending);
  useEffect(() => {
    latest.current = sending;
  });
  const forSend = useCallback(() => latest.current, []);
  const toggle = useCallback(() => {
    if (!available) return;
    setPressed((held) => {
      const next = new Set(held);
      if (!next.delete(key)) next.add(key);
      return next;
    });
  }, [available, key]);

  return { available, reason, model: available ? (availability?.model ?? undefined) : undefined, armed, toggle, forSend };
}
