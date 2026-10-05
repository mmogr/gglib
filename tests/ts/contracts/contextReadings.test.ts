/**
 * The context reading every gglib client draws, replayed from its worked
 * examples.
 *
 * `contracts/context/readings.json` is written by hand, for every gglib
 * client to replay: this test replays it against the page's rule. The file,
 * not prose, says the arithmetic, the thresholds and the words, so every
 * case in it is replayed. A case added there is replayed with no new test
 * here; the guard counts the cases, so its counts are raised with the file.
 *
 * The two halves come by their figures the two ways the page does. A
 * `readings` case is read from the `turn_usage` event that would carry it;
 * a `sources` case is a chat reopened from its saved rows, a reply that did
 * not finish marked as the daemon marks one.
 */

import { describe, it, expect } from 'vitest';
import type { ChatMessage } from '../../../src/services/transport';
import { buildLoadedMessage } from '../../../src/hooks/useChatPersistence/buildLoadedMessage';
import { turnMadeFromUsage } from '../../../src/utils/messages/turnMade';
import {
  contextLines,
  contextReading,
  decidingMade,
  percentText,
  spokenReading,
} from '../../../src/components/ChatMessagesPanel/components/contextReading';
import { rust } from './rustSource';

/** One reply's figures, under the names the wire gives them. */
interface Figures {
  prompt_tokens?: number;
  completion_tokens?: number;
  context_size?: number;
  trimmed_messages?: number;
  finish_reason?: string;
}

interface ReadingCase {
  name: string;
  given: Figures;
  expect: {
    shown: boolean;
    used?: number;
    percent?: number;
    percent_text?: string;
    severity?: string;
    spoken?: string;
    lines?: string[];
  };
}

interface SourceCase {
  name: string;
  replies: Array<Figures & { incomplete?: boolean }>;
  /** The index of the reply whose figures decide; null when none does. */
  decides: number | null;
}

const VECTORS = JSON.parse(rust('contracts/context/readings.json')) as {
  readings: ReadingCase[];
  sources: SourceCase[];
};

/** A chat's rows as the daemon saves them: a question before each reply, a reply's figures under the row's keys. */
function savedChat(replies: SourceCase['replies']): ChatMessage[] {
  return replies.flatMap((reply, index): ChatMessage[] => [
    { id: 2 * index + 1, conversation_id: 1, role: 'user', content: `Question ${index}`, created_at: '2026-10-05T09:00:00Z' },
    {
      id: 2 * index + 2,
      conversation_id: 1,
      role: 'assistant',
      content: `Reply ${index}`,
      created_at: '2026-10-05T09:00:00Z',
      metadata: {
        ...(reply.prompt_tokens !== undefined && { promptTokens: reply.prompt_tokens }),
        ...(reply.completion_tokens !== undefined && { completionTokens: reply.completion_tokens }),
        ...(reply.context_size !== undefined && { contextSize: reply.context_size }),
        ...(reply.trimmed_messages !== undefined && { trimmedMessages: reply.trimmed_messages }),
        ...(reply.finish_reason !== undefined && { finishReason: reply.finish_reason }),
        ...(reply.incomplete && { incomplete: true }),
      },
    },
  ]);
}

describe('contracts/context/readings.json', () => {
  it('holds cases of every kind and every case there is, so a pass is never an empty or a short replay', () => {
    const shown = VECTORS.readings.filter((c) => c.expect.shown);
    expect(new Set(shown.map((c) => c.expect.severity))).toEqual(new Set(['normal', 'warning', 'danger']));
    expect(VECTORS.readings.length).toBeGreaterThan(shown.length);
    expect(VECTORS.sources.some((c) => c.decides === null)).toBe(true);
    expect(VECTORS.sources.some((c) => c.decides !== null && c.replies.length > 1)).toBe(true);
    expect(VECTORS.sources.some((c) => c.replies.some((r) => r.incomplete))).toBe(true);

    // The file's own counts, so one cut short or read in part fails here.
    expect({
      readings: VECTORS.readings.length,
      shown: VECTORS.readings.filter((c) => c.expect.shown === true).length,
      notShown: VECTORS.readings.filter((c) => c.expect.shown === false).length,
      sources: VECTORS.sources.length,
      decided: VECTORS.sources.filter((c) => typeof c.decides === 'number').length,
      undecided: VECTORS.sources.filter((c) => c.decides === null).length,
    }).toEqual({ readings: 20, shown: 16, notShown: 4, sources: 9, decided: 7, undecided: 2 });
  });

  describe('readings: what one reply\'s figures draw and say', () => {
    it.each(VECTORS.readings)('$name', ({ given, expect: want }) => {
      const reading = contextReading(turnMadeFromUsage(given));
      const drawn = reading && {
        shown: true,
        used: reading.used,
        percent: reading.percent,
        percent_text: percentText(reading),
        severity: reading.severity,
        spoken: spokenReading(reading),
        lines: contextLines(reading),
      };
      expect(drawn ?? { shown: false }).toEqual(want);
    });
  });

  describe('sources: which reply decides', () => {
    it.each(VECTORS.sources)('$name', ({ replies, decides }) => {
      const thread = savedChat(replies).map((row) => buildLoadedMessage(row, 1));
      const made = decidingMade(thread);

      // The deciding reply's own figures, and no other reply's.
      const decider = decides === null ? undefined : thread[2 * decides + 1];
      expect(made).toBe((decider?.metadata as { custom?: { made?: unknown } } | undefined)?.custom?.made);
      // What they draw is what that reply alone draws: nothing added up,
      // and nothing borrowed from a reply before it.
      const alone = decides === null ? null : contextReading(turnMadeFromUsage(replies[decides]));
      expect(contextReading(made)).toEqual(alone);
    });
  });
});
