/**
 * What a turn's margin says about it: only what the page has for that turn.
 *
 * A figure the page does not have is left out, never drawn as zero, a dash
 * or a guess. How a turn was made (its model, token counts and times) comes
 * from its `turn_usage` event while drawn and from its saved row once
 * loaded: the same figures, so both say the same. The rate is computed
 * here, tokens written over the time writing; nothing saves it.
 *
 * @module turnFigures
 */

import type { GglibMessageCustom, PromptReading } from '../../../types/messages';
import type { TurnMade } from '../../../utils/messages/turnMade';
import { formatCount } from '../../../utils/format';
import { formatPerSecond } from '../../../utils/formatPerSecond';

/** A turn's seconds: "5.2s" or "1m 23s", as its reasoning block says them. */
export function formatThinkingDuration(seconds: number): string {
  if (seconds < 60) {
    return `${seconds.toFixed(1)}s`;
  }
  const minutes = Math.floor(seconds / 60);
  const remainingSeconds = seconds % 60;
  return `${minutes}m ${remainingSeconds.toFixed(0)}s`;
}

/** The facts about one reply the margin is drawn from. */
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
  /** How the turn was made. */
  made?: TurnMade;
}

/** Who wrote the reply: its model, when known. */
export function replyName(facts: ReplyFacts): string {
  return facts.made?.modelName ?? 'Assistant';
}

/** Tokens written per second of writing; null without both figures. */
export function writingRate(made: TurnMade | undefined): number | null {
  const tokens = made?.completionTokens;
  const ms = made?.writingDurationMs;
  if (tokens == null || ms == null || ms <= 0) return null;
  return tokens / (ms / 1000);
}

/** A reply's figures after it arrived, one line each, in the margin's order. */
export function madeLines(facts: ReplyFacts): string[] {
  const lines: string[] = [];
  if (facts.unfinished) lines.push('unfinished');
  if (facts.thinkingSeconds != null) {
    lines.push(`thought ${formatThinkingDuration(facts.thinkingSeconds)}`);
  }
  if (facts.toolCalls > 0) {
    lines.push(`${facts.toolCalls} tool call${facts.toolCalls === 1 ? '' : 's'}`);
  }
  const made = facts.made;
  if (made?.promptTokens != null) lines.push(`${formatCount(made.promptTokens)} tok read`);
  if (made?.cachedTokens != null) lines.push(`${formatCount(made.cachedTokens)} from cache`);
  const rate = writingRate(made);
  const timing = [
    made?.turnDurationMs != null ? formatThinkingDuration(made.turnDurationMs / 1000) : null,
    rate != null ? `${formatPerSecond(rate)} tok/s` : null,
  ].filter((part): part is string => part !== null);
  if (timing.length > 0) lines.push(timing.join(' · '));
  return lines;
}

/** What a reply still arriving is doing, as its events so far say. */
export type ArrivingPhase =
  | 'Waiting for the model'
  | 'Reading the prompt'
  | 'Starting the reply'
  | 'Thinking'
  | 'Writing'
  | 'Calling tools';

export function arrivingPhase(parts: {
  prompt?: PromptReading;
  hasReasoning: boolean;
  hasText: boolean;
  toolCallsRunning: boolean;
}): ArrivingPhase {
  if (parts.toolCallsRunning) return 'Calling tools';
  if (parts.hasText) return 'Writing';
  if (parts.hasReasoning) return 'Thinking';
  if (parts.prompt && parts.prompt.processed < parts.prompt.total) return 'Reading the prompt';
  if (parts.prompt) return 'Starting the reply';
  return 'Waiting for the model';
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
    made: custom?.made,
  };
}

/** Hours and minutes, as the turn's margin shows a time. */
export function turnTime(at: Date): string {
  return new Intl.DateTimeFormat(undefined, { hour: '2-digit', minute: '2-digit' }).format(at);
}
