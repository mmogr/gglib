/**
 * What a turn says about itself: only what the page has for that turn.
 *
 * A figure the page does not have is left out, never drawn as zero, a dash
 * or a guess. How a turn was made (its model, token counts and times) comes
 * from its `turn_usage` event while drawn and from its saved row once
 * loaded: the same figures, so both say the same. The rate is tokens
 * written over the time writing (`readings/throughput`); nothing saves it.
 *
 * A reply says its figures in three places, each read here: the fold's
 * summary (how long it thought, how many tools it called), the figure line
 * under it (how long it took, how fast it wrote, what the cache saved) and
 * the figure line's tooltip, which has the full set.
 *
 * @module turnFigures
 */

import type { GglibMessageCustom, PromptReading, TurnWaiting } from '../types/messages';
import type { TurnMade } from '../utils/messages/turnMade';
import { formatCount } from '../utils/format';
import { formatPerSecond } from '../utils/formatPerSecond';
import { tokensPerSecond } from './throughput';

/**
 * Seconds as a turn says them: "0.4 s", "4.7 s" under ten, "19 s", "1 min 23 s".
 * Rounded to the precision shown before the unit is chosen, so 9.96 is
 * "10 s" and 59.6 is "1 min 0 s", never "10.0 s" or "60 s".
 */
export function secondsWords(seconds: number): string {
  const tenths = Math.round(seconds * 10) / 10;
  if (tenths < 10) return `${tenths.toFixed(1)} s`;
  const whole = Math.round(seconds);
  if (whole < 60) return `${whole} s`;
  return `${Math.floor(whole / 60)} min ${whole % 60} s`;
}

/** The facts about one reply its figures are drawn from. */
export interface ReplyFacts {
  /** When the row was saved; absent for a turn still being drawn. */
  savedAt?: Date;
  /** How long the turn thought, as the daemon saved it. */
  thinkingSeconds?: number | null;
  toolCalls: number;
  /** The daemon saved it as a reply that did not finish. */
  unfinished: boolean;
  /** How far the prompt was read, from the run's events: only while it arrives. */
  prompt?: PromptReading;
  /** What the turn is waiting for, from the run's events: only while it arrives. */
  waiting?: TurnWaiting;
  /** How the turn was made. */
  made?: TurnMade;
}

/**
 * Tokens written per second of writing; null without both figures, and for
 * a turn of one token or one not streamed, which have no time writing.
 */
export function writingRate(made: TurnMade | undefined): number | null {
  if (made?.completionTokens != null && made.completionTokens < 2) return null;
  return tokensPerSecond(made?.completionTokens, made?.writingDurationMs);
}

function toolCallWords(count: number): string {
  return `${count} tool call${count === 1 ? '' : 's'}`;
}

/**
 * The fold's summary: how long the reply thought, then how many tools it
 * called. While it thinks, "Thinking…" and the seconds so far; `seconds` is
 * the saved or the live figure, whichever the reply has.
 */
export function foldSummary(parts: { thinking: boolean; live: boolean; seconds: number | null; toolCalls: number }): string {
  const words: string[] = [];
  if (parts.live) {
    words.push(parts.seconds != null ? `Thinking… ${secondsWords(parts.seconds)}` : 'Thinking…');
  } else if (parts.thinking) {
    words.push(parts.seconds != null ? `Thought for ${secondsWords(parts.seconds)}` : 'Thought');
  }
  if (parts.toolCalls > 0) words.push(toolCallWords(parts.toolCalls));
  return words.join(' · ');
}

/** The word the figure line starts with for a reply the daemon saved unfinished. */
export const UNFINISHED = 'unfinished';

/**
 * The figure line's parts: whether it finished, how long it took, how fast
 * it wrote, and how much of the prompt the cache saved; none when the reply
 * has none of them.
 */
export function figureParts(facts: ReplyFacts): string[] {
  const made = facts.made;
  const rate = writingRate(made);
  return [
    facts.unfinished ? UNFINISHED : null,
    made?.turnDurationMs != null ? secondsWords(made.turnDurationMs / 1000) : null,
    rate != null ? `${formatPerSecond(rate)} tok/s` : null,
    made?.cachedTokens != null ? `${formatCount(made.cachedTokens)} tokens reused` : null,
  ].filter((part): part is string => part !== null);
}

/** The figure line, as it reads: its parts with " · " between them; empty with none. */
export function figureLine(facts: ReplyFacts): string {
  return figureParts(facts).join(' · ');
}

/** The figure line's tooltip: every figure the reply has, one line each. */
export function figureDetail(facts: ReplyFacts): string[] {
  const made = facts.made;
  const lines: string[] = [];
  if (made?.modelName) lines.push(made.modelQuantization ? `${made.modelName} · ${made.modelQuantization}` : made.modelName);
  if (made?.device) lines.push(`for ${made.device}`);
  if (facts.unfinished) lines.push('unfinished');
  if (made?.finishReason === 'length') lines.push('cut off before it finished');
  if (facts.thinkingSeconds != null) lines.push(`thought for ${secondsWords(facts.thinkingSeconds)}`);
  if (facts.toolCalls > 0) lines.push(toolCallWords(facts.toolCalls));
  if (made?.promptTokens != null) lines.push(`${formatCount(made.promptTokens)} tokens read`);
  if (made?.cachedTokens != null) lines.push(`${formatCount(made.cachedTokens)} tokens reused`);
  if (made?.completionTokens != null) lines.push(`${formatCount(made.completionTokens)} tokens written`);
  const rate = writingRate(made);
  const timing = [
    made?.turnDurationMs != null ? secondsWords(made.turnDurationMs / 1000) : null,
    rate != null ? `${formatPerSecond(rate)} tok/s` : null,
  ].filter((part): part is string => part !== null);
  if (timing.length > 0) lines.push(timing.join(' · '));
  return lines;
}

/**
 * The model a reply names above itself: its own, only where the reply
 * before it was another's (`previous` is that reply's model, `null` when it
 * is not known, `undefined` when there is no reply before). The first reply
 * names none, since the composer names the model, nor does one whose model
 * is not known.
 */
export function modelShown(facts: ReplyFacts, previous: string | null | undefined): string | null {
  const name = facts.made?.modelName;
  if (!name || previous === undefined) return null;
  return previous === name ? null : name;
}

/**
 * What a reply with no token yet is waiting for, as its events so far say.
 * Once a token comes the fold and the text say what it is doing.
 */
export type ArrivingPhase =
  | 'Waiting for the model'
  | 'Queued behind an image render'
  | 'Waiting for the model to load'
  | 'Reading the prompt'
  | 'Starting the reply';

/**
 * A wait is said only until the model reads the prompt: a prompt reading
 * that comes after it takes it off the turn, so one still there is the
 * newer of the two.
 */
export function arrivingPhase(parts: { prompt?: PromptReading; waiting?: TurnWaiting }): ArrivingPhase {
  if (parts.waiting) {
    return parts.waiting.reason === 'image_render' ? 'Queued behind an image render' : 'Waiting for the model to load';
  }
  if (parts.prompt && parts.prompt.processed < parts.prompt.total) return 'Reading the prompt';
  if (parts.prompt) return 'Starting the reply';
  return 'Waiting for the model';
}

/**
 * How far the render a reply is queued behind has got, and the reply's
 * place in line when it is not next: each only when the wait says it.
 */
export function waitingLines(waiting: TurnWaiting): string[] {
  if (waiting.reason !== 'image_render') return [];
  const lines: string[] = [];
  if (waiting.total > 0) lines.push(`step ${waiting.step} of ${waiting.total}`);
  if (waiting.position > 1) lines.push(`${waiting.position} in line`);
  return lines;
}

/**
 * The arrival line's figures after its phase: how far the prompt is read
 * while it is read, how much was read once it is, and what the cache gave.
 */
export function promptLines(prompt: PromptReading, phase: ArrivingPhase): string[] {
  return [
    phase === 'Reading the prompt'
      ? `${formatCount(prompt.processed)} of ${formatCount(prompt.total)} tokens read`
      : `${formatCount(prompt.total)} tokens read`,
    `${formatCount(prompt.cached)} from cache`,
  ];
}

/** The message shape the facts are read from: assistant-ui's, loosely. */
interface MessageLike {
  id: string;
  createdAt?: Date;
  content: readonly unknown[];
  status?: { type: string };
  metadata?: unknown;
}

/** A reply's facts, from its message. Only a saved row has a time. */
export function replyFacts(message: MessageLike): ReplyFacts {
  const custom = (message.metadata as { custom?: GglibMessageCustom } | undefined)?.custom;
  const saved = message.id.startsWith('db-');
  const toolCalls = message.content.filter(
    (part) => typeof part === 'object' && part !== null && (part as { type?: unknown }).type === 'tool-call',
  ).length;
  return {
    savedAt: saved ? message.createdAt : undefined,
    thinkingSeconds: custom?.thinkingDurationSeconds,
    toolCalls,
    unfinished: saved && message.status?.type === 'incomplete',
    prompt: custom?.prompt,
    waiting: custom?.waiting,
    made: custom?.made,
  };
}

/** Hours and minutes, as a turn shows its time on hover. */
export function turnTime(at: Date): string {
  return new Intl.DateTimeFormat(undefined, { hour: '2-digit', minute: '2-digit' }).format(at);
}
