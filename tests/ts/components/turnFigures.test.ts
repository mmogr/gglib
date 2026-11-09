/**
 * A reply's figures: only what the page has for that turn, in three
 * places (the fold's summary, the figure line and its tooltip's full set).
 * A reply that cannot start says what it waits for, and for a render the
 * step it is on.
 */

import { describe, it, expect } from 'vitest';
import {
  arrivingPhase,
  figureDetail,
  figureParts,
  foldSummary,
  modelShown,
  promptLines,
  replyFacts,
  secondsWords,
} from '../../../src/readings/turnFigures';

const MADE = {
  modelName: 'Qwen3.8-27B',
  modelQuantization: 'Q8_0',
  promptTokens: 3180,
  cachedTokens: 2100,
  completionTokens: 496,
  turnDurationMs: 41_000,
  writingDurationMs: 40_992,
  /** The daemon's rate for these counts: 496 tokens over 40.992 s. */
  tokensPerSecond: 12.09992193598751,
};

describe('secondsWords', () => {
  it('says tenths under ten seconds, whole seconds under a minute, then minutes', () => {
    expect(secondsWords(0.4)).toBe('0.4 s');
    expect(secondsWords(3)).toBe('3 s');
    expect(secondsWords(2.96)).toBe('3 s');
    expect(secondsWords(4.74)).toBe('4.7 s');
    expect(secondsWords(19)).toBe('19 s');
    expect(secondsWords(83)).toBe('1 min 23 s');
  });

  it('rounds to the precision it shows before it chooses the unit', () => {
    expect(secondsWords(9.95)).toBe('10 s');
    expect(secondsWords(9.94)).toBe('9.9 s');
    expect(secondsWords(9.96)).toBe('10 s');
    expect(secondsWords(59.5)).toBe('1 min');
    expect(secondsWords(59.4)).toBe('59 s');
    expect(secondsWords(59.6)).toBe('1 min');
    expect(secondsWords(60)).toBe('1 min');
    expect(secondsWords(61)).toBe('1 min 1 s');
    expect(secondsWords(90)).toBe('1 min 30 s');
    expect(secondsWords(119.6)).toBe('2 min');
    expect(secondsWords(3599.6)).toBe('60 min');
  });
});

describe('foldSummary', () => {
  it('says how long it thought, then the tool calls, singular and plural', () => {
    expect(foldSummary({ thinking: true, live: false, seconds: 3, toolCalls: 2 })).toBe('Thought for 3 s · 2 tool calls');
    expect(foldSummary({ thinking: true, live: false, seconds: 3, toolCalls: 1 })).toBe('Thought for 3 s · 1 tool call');
    expect(foldSummary({ thinking: false, live: false, seconds: null, toolCalls: 1 })).toBe('1 tool call');
    expect(foldSummary({ thinking: true, live: false, seconds: null, toolCalls: 0 })).toBe('Thought');
  });

  it('is live while it thinks', () => {
    expect(foldSummary({ thinking: true, live: true, seconds: 4.2, toolCalls: 0 })).toBe('Thinking… 4.2 s');
    expect(foldSummary({ thinking: true, live: true, seconds: null, toolCalls: 0 })).toBe('Thinking…');
  });
});

describe('figureParts', () => {
  /** The parts as the line reads them, with the dots between. */
  const line = (facts: Parameters<typeof figureParts>[0]) => figureParts(facts).join(' · ');

  it('says nothing it was not given', () => {
    expect(line({ toolCalls: 0, unfinished: false })).toBe('');
    expect(line({ toolCalls: 2, unfinished: false, thinkingSeconds: 3 })).toBe('');
  });

  it('says how long it took, how fast it wrote and what the cache saved', () => {
    expect(line({ toolCalls: 0, unfinished: false, made: MADE })).toBe('41 s · 12 t/s · 2,100 tokens reused');
    expect(line({ toolCalls: 0, unfinished: true, made: { turnDurationMs: 900 } })).toBe('unfinished · 0.9 s');
    expect(line({ toolCalls: 0, unfinished: false, made: { cachedTokens: 0 } })).toBe('0 tokens reused');
  });

  it('says the daemon\'s rate, one decimal under ten, and none where it sent none', () => {
    expect(line({ toolCalls: 0, unfinished: false, made: { tokensPerSecond: 8.46 } })).toBe('8.5 t/s');
    expect(line({ toolCalls: 0, unfinished: false, made: { tokensPerSecond: 8.25 } })).toBe('8.3 t/s');
    // The counts a rate is made from, with no rate: the page divides nothing.
    expect(line({ toolCalls: 0, unfinished: false, made: { completionTokens: 50, writingDurationMs: 4000, turnDurationMs: 300 } })).toBe('0.3 s');
  });
});

describe('figureDetail', () => {
  it('has every figure the reply has, one line each', () => {
    expect(
      figureDetail({
        unfinished: true,
        thinkingSeconds: 19,
        toolCalls: 2,
        made: { ...MADE, device: 'phone-7c2e', finishReason: 'length' },
      }),
    ).toEqual([
      'Qwen3.8-27B · Q8_0',
      'for phone-7c2e',
      'unfinished',
      'cut off before it finished',
      'thought for 19 s',
      '2 tool calls',
      '3,180 tokens read',
      '2,100 tokens reused',
      '496 tokens written',
      '41 s · 12 t/s',
    ]);
  });

  it('leaves out each figure the turn lacks', () => {
    expect(figureDetail({ toolCalls: 0, unfinished: false })).toEqual([]);
    expect(figureDetail({ toolCalls: 1, unfinished: false, made: { modelName: 'qwen3' } })).toEqual(['qwen3', '1 tool call']);
  });
});

describe('modelShown', () => {
  const facts = { toolCalls: 0, unfinished: false, made: { modelName: 'qwen3' } };

  it('names the model only where the reply before was another model\'s', () => {
    expect(modelShown(facts, 'llama-3.2-3b')).toBe('qwen3');
    expect(modelShown(facts, null)).toBe('qwen3');
    expect(modelShown(facts, 'qwen3')).toBeNull();
  });

  it('names none for the first reply, nor for one whose model is not known', () => {
    expect(modelShown(facts, undefined)).toBeNull();
    expect(modelShown({ toolCalls: 0, unfinished: false }, 'qwen3')).toBeNull();
  });
});

describe('promptLines', () => {
  it('says how far the prompt is read while it is, then how much it was, and the cache', () => {
    const prompt = { processed: 1240, total: 3420, cached: 2100 };
    expect(promptLines(prompt, 'Reading the prompt')).toEqual(['1,240 of 3,420 tokens read', '2,100 from cache']);
    expect(promptLines(prompt, 'Starting the reply')).toEqual(['3,420 tokens read', '2,100 from cache']);
  });
});

describe('replyFacts', () => {
  const at = new Date('2026-09-01T09:13:00Z');

  it('gives a time only to a saved row', () => {
    expect(replyFacts({ id: 'db-12', createdAt: at, content: [] }).savedAt).toBe(at);
    expect(replyFacts({ id: 'drawn-1', createdAt: at, content: [] }).savedAt).toBeUndefined();
  });

  it('counts tool calls and reads what the row saved', () => {
    const facts = replyFacts({
      id: 'db-12',
      content: [{ type: 'reasoning', text: 'x' }, { type: 'tool-call' }, { type: 'text', text: 'y' }],
      status: { type: 'incomplete' },
      metadata: { custom: { thinkingDurationSeconds: 4.5 } },
    });
    expect(facts).toMatchObject({ toolCalls: 1, unfinished: true, thinkingSeconds: 4.5 });
  });

  it('calls a reply unfinished only when the daemon saved it so', () => {
    expect(replyFacts({ id: 'drawn-1', content: [], status: { type: 'incomplete' } }).unfinished).toBe(false);
  });
});

describe('arrivingPhase', () => {
  const none = {};

  it('follows the events so far', () => {
    expect(arrivingPhase(none)).toBe('Waiting for the model');
    expect(arrivingPhase({ ...none, prompt: { processed: 1, total: 2, cached: 0 } })).toBe('Reading the prompt');
    expect(arrivingPhase({ ...none, prompt: { processed: 2, total: 2, cached: 0 } })).toBe('Starting the reply');
  });
});

describe('a reply that waits', () => {
  const none = {};
  const load = { reason: 'model_load' as const, step: 0, total: 0, position: 0, said: 'Loading Qwen3-4B… 12 s' };

  it("says the daemon's words for what it waits for, until the model reads the prompt", () => {
    expect(arrivingPhase({ ...none, waiting: load })).toBe('Loading Qwen3-4B… 12 s');
    // A wait still on the turn is newer than its prompt reading, which takes an older one off.
    expect(arrivingPhase({ ...none, waiting: load, prompt: { processed: 2, total: 2, cached: 0 } })).toBe(
      'Loading Qwen3-4B… 12 s',
    );
  });

  it('says only that it waits for a wait from a daemon too old to word it', () => {
    expect(arrivingPhase({ ...none, waiting: { reason: 'model_load', step: 0, total: 0, position: 0 } })).toBe(
      'Waiting for the model',
    );
  });

  it('is read from the turn drawn from a run', () => {
    const message = { id: 'live', content: [], metadata: { custom: { waiting: load } } };
    expect(replyFacts(message).waiting).toEqual(load);
    expect(replyFacts({ id: 'live', content: [] }).waiting).toBeUndefined();
  });
});
