/**
 * What a turn's margin says about it: only what the page has for that turn.
 *
 * A figure the page does not have is left out, never drawn as zero, a dash
 * or a guess. Saved rows carry their time, how long the turn thought and
 * its tool calls; a turn drawn from a run also carries how far its prompt
 * was read, until the run ends and the saved rows replace it. Nothing on
 * this side knows a saved reply's model, its token counts or its speed.
 *
 * @module turnFigures
 */

import type { GglibMessageCustom, PromptReading } from '../../../types/messages';
import { formatCount } from '../../../utils/format';

/** How long a turn thought: "5.2s" or "1m 23s", as its reasoning block says it. */
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
  /** How far the prompt was read, from the run's events. */
  prompt?: PromptReading;
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
  if (facts.prompt) {
    lines.push(`${formatCount(facts.prompt.total)} tok read`);
    lines.push(`${formatCount(facts.prompt.cached)} from cache`);
  }
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
  };
}

/** Hours and minutes, as the turn's margin shows a time. */
export function turnTime(at: Date): string {
  return new Intl.DateTimeFormat(undefined, { hour: '2-digit', minute: '2-digit' }).format(at);
}
