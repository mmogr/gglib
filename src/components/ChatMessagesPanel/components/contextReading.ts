/**
 * How much of the model's context a conversation has used: the reading the
 * composer's ring draws, and the sentences its detail says.
 *
 * The rule is the one every gglib client follows, worked through in
 * `contracts/context/readings.json`. The newest reply decides, from its last
 * model call: counts are never added up across replies. A reply that did not
 * finish and carries no counts is passed over, so a reply stopped before its
 * counts came leaves the reading before it. The reading exists only when
 * that one reply has its tokens read, its tokens written and its context's
 * size; a size is never borrowed from an older reply or from the catalogue.
 * Without all three there is no reading, and nothing is drawn in its place.
 *
 * @module contextReading
 */

import type { GglibMessageCustom } from '../../../types/messages';
import type { TurnMade } from '../../../utils/messages/turnMade';
import { formatCount } from '../../../utils/format';
import { usagePercent, usageSeverity, type UsageSeverity } from '../../../utils/contextUsage';

/** A conversation's context reading, from one reply's figures. */
export interface ContextReading {
  /** Tokens read plus tokens written, by the reply's last model call. */
  used: number;
  /** The context, in tokens, the server that answered was launched with. */
  size: number;
  /** `used` of `size`, whole; never over 100. */
  percent: number;
  severity: UsageSeverity;
  /** Earlier messages shortened or left out so the request fit; 0 for none. */
  trimmed: number;
  /** The reply stopped at the model's limit, not at its own end. */
  cutOff: boolean;
}

/** The message shape a reading is found in: assistant-ui's, loosely. */
interface MessageLike {
  role: string;
  status?: { type: string };
  metadata?: unknown;
}

/**
 * The figures of the reply that decides the reading: the newest reply's,
 * passing over one that did not finish (still arriving, or saved as cut
 * short) and has neither count. Undefined when no reply decides, or the one
 * that does has no figures.
 */
export function decidingMade(messages: readonly MessageLike[]): TurnMade | undefined {
  for (let i = messages.length - 1; i >= 0; i -= 1) {
    const message = messages[i];
    if (message.role !== 'assistant') continue;
    const made = (message.metadata as { custom?: GglibMessageCustom } | undefined)?.custom?.made;
    const counted = made?.promptTokens != null || made?.completionTokens != null;
    const unfinished = message.status != null && message.status.type !== 'complete';
    if (counted || !unfinished) return made;
  }
  return undefined;
}

/** The reading one reply's figures give; null unless it has both counts and a size above zero. */
export function contextReading(made: TurnMade | null | undefined): ContextReading | null {
  const { promptTokens, completionTokens, contextSize } = made ?? {};
  if (promptTokens == null || completionTokens == null || contextSize == null || !(contextSize > 0)) return null;
  const used = promptTokens + completionTokens;
  const percent = usagePercent(used, contextSize);
  return {
    used,
    size: contextSize,
    percent,
    severity: usageSeverity(percent),
    trimmed: made?.trimmedMessages ?? 0,
    cutOff: made?.finishReason === 'length',
  };
}

/** The percent as it is written: a reading under one percent says so, never "0%". */
export function percentText(reading: ContextReading): string {
  return reading.percent === 0 ? '<1%' : `${reading.percent}%`;
}

/** The reading as it is spoken: the figure, then how near full it is. */
export function spokenReading(reading: ContextReading): string {
  const figure = reading.percent === 0 ? 'less than 1' : `${reading.percent}`;
  const near = reading.severity === 'danger' ? ', almost full' : reading.severity === 'warning' ? ', filling up' : '';
  return `${figure} percent of context used${near}`;
}

/** The detail's sentences, in the order they are read. */
export function contextLines(reading: ContextReading): string[] {
  const lines = [
    `${formatCount(reading.used)} of ${formatCount(reading.size)} tokens (${percentText(reading)}) after the last finished reply.`,
  ];
  if (reading.severity === 'warning') lines.push('Context is filling up.');
  if (reading.severity === 'danger') lines.push('Context is almost full.');
  if (reading.trimmed === 1) lines.push('1 earlier message was shortened or left out to fit.');
  if (reading.trimmed > 1) {
    lines.push(`${formatCount(reading.trimmed)} earlier messages were shortened or left out to fit.`);
  }
  if (reading.cutOff) lines.push('The last reply was cut off before it finished.');
  return lines;
}
