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
  figureLine,
  foldSummary,
  modelShown,
  promptLines,
  replyFacts,
  secondsWords,
  waitingLines,
  writingRate,
} from '../../../src/readings/turnFigures';

const MADE = {
  modelName: 'Qwen3.8-27B',
  modelQuantization: 'Q8_0',
  promptTokens: 3180,
  cachedTokens: 2100,
  completionTokens: 496,
  turnDurationMs: 41_000,
  writingDurationMs: 40_992,
};

describe('secondsWords', () => {
  it('says tenths under ten seconds, whole seconds under a minute, then minutes', () => {
    expect(secondsWords(0.4)).toBe('0.4 s');
    expect(secondsWords(4.74)).toBe('4.7 s');
    expect(secondsWords(19)).toBe('19 s');
    expect(secondsWords(83)).toBe('1 min 23 s');
  });
});

describe('foldSummary', () => {
  it('says how long it thought, then the tool calls, singular and plural', () => {
    expect(foldSummary({ thinking: true, live: false, seconds: 3, toolCalls: 2 })).toBe('Thought for 3.0 s · 2 tool calls');
    expect(foldSummary({ thinking: true, live: false, seconds: 3, toolCalls: 1 })).toBe('Thought for 3.0 s · 1 tool call');
    expect(foldSummary({ thinking: false, live: false, seconds: null, toolCalls: 1 })).toBe('1 tool call');
    expect(foldSummary({ thinking: true, live: false, seconds: null, toolCalls: 0 })).toBe('Thought');
  });

  it('is live while it thinks', () => {
    expect(foldSummary({ thinking: true, live: true, seconds: 4.2, toolCalls: 0 })).toBe('Thinking… 4.2 s');
    expect(foldSummary({ thinking: true, live: true, seconds: null, toolCalls: 0 })).toBe('Thinking…');
  });
});

describe('figureLine', () => {
  it('says nothing it was not given', () => {
    expect(figureLine({ toolCalls: 0, unfinished: false })).toBe('');
    expect(figureLine({ toolCalls: 2, unfinished: false, thinkingSeconds: 3 })).toBe('');
  });

  it('says how long it took, how fast it wrote and what the cache saved', () => {
    expect(figureLine({ toolCalls: 0, unfinished: false, made: MADE })).toBe('41 s · 12 tok/s · 2,100 tokens reused');
    expect(figureLine({ toolCalls: 0, unfinished: true, made: { turnDurationMs: 900 } })).toBe('unfinished · 0.9 s');
    expect(figureLine({ toolCalls: 0, unfinished: false, made: { cachedTokens: 0 } })).toBe('0 tokens reused');
  });

  it('says no rate for a turn of one token, nor one not streamed', () => {
    expect(figureLine({ toolCalls: 0, unfinished: false, made: { completionTokens: 1, writingDurationMs: 0, turnDurationMs: 300 } })).toBe('0.3 s');
    expect(figureLine({ toolCalls: 0, unfinished: false, made: { completionTokens: 1, writingDurationMs: 40 } })).toBe('');
    expect(figureLine({ toolCalls: 0, unfinished: false, made: { completionTokens: 50 } })).toBe('');
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
      '41 s · 12 tok/s',
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
  const none = { hasReasoning: false, hasText: false, toolCallsRunning: false };

  it('follows the events so far', () => {
    expect(arrivingPhase(none)).toBe('Waiting for the model');
    expect(arrivingPhase({ ...none, prompt: { processed: 1, total: 2, cached: 0 } })).toBe('Reading the prompt');
    expect(arrivingPhase({ ...none, prompt: { processed: 2, total: 2, cached: 0 } })).toBe('Starting the reply');
    expect(arrivingPhase({ ...none, hasReasoning: true })).toBe('Thinking');
    expect(arrivingPhase({ ...none, hasReasoning: true, hasText: true })).toBe('Writing');
    expect(arrivingPhase({ ...none, hasText: true, toolCallsRunning: true })).toBe('Calling tools');
  });
});

describe('a reply that waits', () => {
  const none = { hasReasoning: false, hasText: false, toolCallsRunning: false };
  const render = { reason: 'image_render' as const, step: 3, total: 20, position: 1 };
  const load = { reason: 'model_load' as const, step: 0, total: 0, position: 0 };

  it('says what it waits for, until the model writes, thinks or calls a tool', () => {
    expect(arrivingPhase({ ...none, waiting: render })).toBe('Queued behind an image render');
    expect(arrivingPhase({ ...none, waiting: load })).toBe('Waiting for the model to load');
    // A wait still on the turn is newer than its prompt reading, which takes an older one off.
    expect(arrivingPhase({ ...none, waiting: render, prompt: { processed: 2, total: 2, cached: 0 } })).toBe(
      'Queued behind an image render',
    );
    expect(arrivingPhase({ ...none, waiting: render, hasReasoning: true })).toBe('Thinking');
    expect(arrivingPhase({ ...none, waiting: render, hasText: true })).toBe('Writing');
    expect(arrivingPhase({ ...none, waiting: render, toolCallsRunning: true })).toBe('Calling tools');
  });

  it('says the step the render is on, and its place in line only when it is not next', () => {
    expect(waitingLines(render)).toEqual(['step 3 of 20']);
    expect(waitingLines({ ...render, position: 2 })).toEqual(['step 3 of 20', '2 in line']);
    // A render that has reported no step yet has no count to say.
    expect(waitingLines({ ...render, step: 0, total: 0 })).toEqual([]);
    expect(waitingLines({ ...render, step: 0, total: 20 })).toEqual(['step 0 of 20']);
    expect(waitingLines({ ...load, total: 20, position: 3 })).toEqual([]);
  });

  it('is read from the turn drawn from a run', () => {
    const message = { id: 'live', content: [], metadata: { custom: { waiting: render } } };
    expect(replyFacts(message).waiting).toEqual(render);
    expect(replyFacts({ id: 'live', content: [] }).waiting).toBeUndefined();
  });
});

describe('writingRate', () => {
  it('is tokens written over seconds writing', () => {
    expect(writingRate({ completionTokens: 50, writingDurationMs: 4000 })).toBe(12.5);
    expect(writingRate({ completionTokens: 50 })).toBeNull();
  });
});
