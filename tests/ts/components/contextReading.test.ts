/**
 * The conversation's context reading: which reply's figures decide it, when
 * there is one, and what its detail says.
 *
 * The shared worked examples are replayed in
 * `tests/ts/contracts/contextReadings.test.ts`. These are the page's own
 * cases: a reply still arriving, and the edges the examples leave to it.
 */

import { describe, it, expect } from 'vitest';
import {
  contextLines,
  contextReading,
  decidingMade,
  percentText,
  spokenReading,
} from '../../../src/components/ChatMessagesPanel/components/contextReading';
import type { TurnMade } from '../../../src/utils/messages/turnMade';

const FIRST: TurnMade = { promptTokens: 8000, completionTokens: 200, contextSize: 32768 };
const SECOND: TurnMade = { promptTokens: 9000, completionTokens: 300, contextSize: 32768 };

const user = { role: 'user', metadata: { custom: {} } };
const reply = (made?: TurnMade, status = 'complete') => ({
  role: 'assistant',
  status: { type: status },
  metadata: { custom: made ? { made } : {} },
});

describe('decidingMade', () => {
  it('is the newest reply\'s figures, and no reply\'s when there are none', () => {
    expect(decidingMade([])).toBeUndefined();
    expect(decidingMade([user])).toBeUndefined();
    expect(decidingMade([user, reply(FIRST), user, reply(SECOND)])).toBe(SECOND);
    expect(decidingMade([user, reply(FIRST), user, reply(SECOND), user])).toBe(SECOND);
  });

  it('passes over a reply still arriving until its figures come', () => {
    expect(decidingMade([user, reply(FIRST), user, reply(undefined, 'running')])).toBe(FIRST);
    expect(decidingMade([user, reply(FIRST), user, reply(SECOND, 'running')])).toBe(SECOND);
  });

  it('passes over every unfinished reply without counts, however many', () => {
    const thread = [user, reply(FIRST), user, reply(undefined, 'incomplete'), user, reply({ modelName: 'qwen3' }, 'incomplete')];
    expect(decidingMade(thread)).toBe(FIRST);
  });

  it('lets a finished reply decide though it has no figures: an older reply\'s are not used', () => {
    expect(decidingMade([user, reply(FIRST), user, reply()])).toBeUndefined();
    const named: TurnMade = { modelName: 'qwen3' };
    expect(decidingMade([user, reply(FIRST), user, reply(named)])).toBe(named);
    expect(contextReading(named)).toBeNull();
  });

  it('lets an unfinished reply with one count decide, and it gives no reading', () => {
    const half: TurnMade = { promptTokens: 9000 };
    expect(decidingMade([user, reply(FIRST), user, reply(half, 'incomplete')])).toBe(half);
    expect(contextReading(half)).toBeNull();
  });

  it('reads a row with no status as finished', () => {
    const bare = { role: 'assistant', metadata: { custom: {} } };
    expect(decidingMade([user, reply(FIRST), user, bare])).toBeUndefined();
  });
});

describe('contextReading', () => {
  it('is that one reply\'s tokens read plus written, of its own context', () => {
    expect(contextReading(SECOND)).toEqual({
      used: 9300,
      size: 32768,
      percent: 28,
      severity: 'normal',
      trimmed: 0,
      cutOff: false,
    });
  });

  it('is nothing without both counts and a size above zero', () => {
    expect(contextReading(undefined)).toBeNull();
    expect(contextReading(null)).toBeNull();
    expect(contextReading({ promptTokens: 8000, completionTokens: 200 })).toBeNull();
    expect(contextReading({ promptTokens: 8000, contextSize: 32768 })).toBeNull();
    expect(contextReading({ completionTokens: 200, contextSize: 32768 })).toBeNull();
    expect(contextReading({ ...FIRST, contextSize: 0 })).toBeNull();
    expect(contextReading({ ...FIRST, contextSize: -1 })).toBeNull();
  });

  it('takes the severity from the whole percent, so an exact half is a warning in colour and in words', () => {
    const half = contextReading({ promptTokens: 130, completionTokens: 9, contextSize: 200 })!;
    expect(half).toMatchObject({ percent: 70, severity: 'warning' });
    expect(spokenReading(half)).toBe('70 percent of context used, filling up');
    expect(contextLines(half)[1]).toBe('Context is filling up.');
  });

  it('keeps the counts true past full, and draws full', () => {
    const over = contextReading({ promptTokens: 32900, completionTokens: 100, contextSize: 32768 })!;
    expect(over).toMatchObject({ used: 33000, percent: 100, severity: 'danger' });
  });

  it('says a reply was cut off only when it stopped at the model\'s limit', () => {
    expect(contextReading({ ...FIRST, finishReason: 'length' })!.cutOff).toBe(true);
    expect(contextReading({ ...FIRST, finishReason: 'stop' })!.cutOff).toBe(false);
    expect(contextReading({ ...FIRST, finishReason: 'tool_calls' })!.cutOff).toBe(false);
  });
});

describe('the detail\'s sentences', () => {
  const lines = (made: TurnMade) => contextLines(contextReading(made)!);

  it('says the counts, then how full, then what was trimmed, then a cut-off, in that order', () => {
    expect(lines({ promptTokens: 32000, completionTokens: 768, contextSize: 32768, trimmedMessages: 12, finishReason: 'length' })).toEqual([
      '32,768 of 32,768 tokens (100%) after the last finished reply.',
      'Context is almost full.',
      '12 earlier messages were shortened or left out to fit.',
      'The last reply was cut off before it finished.',
    ]);
  });

  it('says one trimmed message in the singular, and nothing of none', () => {
    expect(lines({ ...FIRST, trimmedMessages: 1 })[1]).toBe('1 earlier message was shortened or left out to fit.');
    expect(lines({ ...FIRST, trimmedMessages: 2 })[1]).toBe('2 earlier messages were shortened or left out to fit.');
    expect(lines({ ...FIRST, trimmedMessages: 0 })).toHaveLength(1);
    expect(lines(FIRST)).toEqual(['8,200 of 32,768 tokens (25%) after the last finished reply.']);
  });

  it('says under one percent as that, never as nought', () => {
    const faint = contextReading({ promptTokens: 100, completionTokens: 20, contextSize: 131072 })!;
    expect(faint.percent).toBe(0);
    expect(percentText(faint)).toBe('<1%');
    expect(spokenReading(faint)).toBe('less than 1 percent of context used');
    expect(contextLines(faint)).toEqual(['120 of 131,072 tokens (<1%) after the last finished reply.']);
  });

  it('says how near full in the spoken reading, and nothing while it is not', () => {
    expect(spokenReading(contextReading(FIRST)!)).toBe('25 percent of context used');
    expect(spokenReading(contextReading({ promptTokens: 29000, completionTokens: 492, contextSize: 32768 })!)).toBe(
      '90 percent of context used, almost full',
    );
  });
});
