/**
 * A turn's margin figures: only what the page has for that turn.
 */

import { describe, it, expect } from 'vitest';
import {
  arrivingPhase,
  madeLines,
  replyFacts,
  replyName,
  writingRate,
} from '../../../src/components/ChatMessagesPanel/components/turnFigures';

describe('madeLines', () => {
  it('says nothing it was not given', () => {
    expect(madeLines({ toolCalls: 0, unfinished: false })).toEqual([]);
    expect(madeLines({ toolCalls: 0, unfinished: false, thinkingSeconds: null })).toEqual([]);
  });

  it('says each figure it has, in the margin order', () => {
    expect(
      madeLines({
        unfinished: true,
        thinkingSeconds: 19,
        toolCalls: 2,
        made: {
          modelName: 'Qwen3.8-27B',
          modelQuantization: 'Q8_0',
          promptTokens: 3180,
          cachedTokens: 2100,
          completionTokens: 496,
          turnDurationMs: 41_000,
          writingDurationMs: 40_992,
        },
      }),
    ).toEqual(['unfinished', 'thought 19.0s', '2 tool calls', '3,180 tok read', '2,100 from cache', '41.0s · 12 tok/s']);
  });

  it('leaves out each figure the turn lacks, and the rate without both of its parts', () => {
    expect(madeLines({ toolCalls: 0, unfinished: false, made: { turnDurationMs: 900 } })).toEqual(['0.9s']);
    expect(madeLines({ toolCalls: 0, unfinished: false, made: { completionTokens: 5 } })).toEqual([]);
    expect(
      madeLines({ toolCalls: 0, unfinished: false, made: { completionTokens: 5, writingDurationMs: 0 } }),
    ).toEqual([]);
    expect(madeLines({ toolCalls: 0, unfinished: false, made: { cachedTokens: 0 } })).toEqual(['0 from cache']);
  });

  it('says nothing of the prompt reading once the turn has arrived', () => {
    expect(
      madeLines({ toolCalls: 0, unfinished: false, prompt: { processed: 10, total: 10, cached: 4 } }),
    ).toEqual([]);
  });

  it('says how long it thought as the reasoning block does, not rounded up', () => {
    expect(madeLines({ toolCalls: 1, unfinished: false, thinkingSeconds: 0.4 })).toEqual([
      'thought 0.4s',
      '1 tool call',
    ]);
    expect(madeLines({ toolCalls: 0, unfinished: false, thinkingSeconds: 83 })).toEqual(['thought 1m 23s']);
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

describe('replyName and writingRate', () => {
  it('names the model when known, and the assistant when not', () => {
    expect(replyName({ toolCalls: 0, unfinished: false, made: { modelName: 'qwen3' } })).toBe('qwen3');
    expect(replyName({ toolCalls: 0, unfinished: false })).toBe('Assistant');
  });

  it('is tokens written over seconds writing', () => {
    expect(writingRate({ completionTokens: 50, writingDurationMs: 4000 })).toBe(12.5);
    expect(writingRate({ completionTokens: 50 })).toBeNull();
  });
});
