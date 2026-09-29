/**
 * A turn's margin figures: only what the page has for that turn.
 */

import { describe, it, expect } from 'vitest';
import {
  arrivingPhase,
  madeLines,
  replyFacts,
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
        prompt: { processed: 3420, total: 3420, cached: 2100 },
      }),
    ).toEqual(['unfinished', 'thought 19s', '2 tool calls', '3,420 tok read', '2,100 from cache']);
  });

  it('does not round a short think up to a second', () => {
    expect(madeLines({ toolCalls: 1, unfinished: false, thinkingSeconds: 0.4 })).toEqual([
      'thought under 1s',
      '1 tool call',
    ]);
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
