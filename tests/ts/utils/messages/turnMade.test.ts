/**
 * A `turn_usage` frame, and a saved row's metadata, read into one shape:
 * only the figures present, whatever else is missing or malformed.
 */

import { describe, it, expect } from 'vitest';
import { turnMadeFromMetadata, turnMadeFromUsage } from '../../../../src/utils/messages/turnMade';

describe('turnMadeFromUsage', () => {
  it('reads a frame with no model, as /api/agent/chat sends it', () => {
    expect(turnMadeFromUsage({ prompt_tokens: 30, completion_tokens: 2, duration_ms: 900 })).toEqual({
      promptTokens: 30,
      completionTokens: 2,
      turnDurationMs: 900,
    });
  });

  it('gives nothing for a frame with no figures, and skips what is not a figure', () => {
    expect(turnMadeFromUsage({})).toBeUndefined();
    const odd = { model: '', cached_tokens: null, writing_ms: 'soon', duration_ms: 5 } as unknown as Parameters<
      typeof turnMadeFromUsage
    >[0];
    expect(turnMadeFromUsage(odd)).toEqual({ turnDurationMs: 5 });
  });

  it('names the paired device whose turn it answered, and only that', () => {
    expect(turnMadeFromUsage({ duration_ms: 5, device: 'phone-7c2e' })).toEqual({
      turnDurationMs: 5,
      device: 'phone-7c2e',
    });
    expect(turnMadeFromMetadata({ device: '' })).toBeUndefined();
  });

  it("reads the context's size, the messages trimmed and why it stopped, the same from a frame and a row", () => {
    const made = { turnDurationMs: 5, finishReason: 'length', contextSize: 8192, trimmedMessages: 3 };
    expect(
      turnMadeFromUsage({ duration_ms: 5, finish_reason: 'length', context_size: 8192, trimmed_messages: 3 }),
    ).toEqual(made);
    expect(turnMadeFromMetadata(made)).toEqual(made);
  });

  it('leaves out each of the three the turn did not have, and a blank, a null and a number written as text', () => {
    expect(turnMadeFromUsage({ duration_ms: 5 })).toEqual({ turnDurationMs: 5 });
    const odd = { duration_ms: 5, finish_reason: '', context_size: null, trimmed_messages: '3' } as unknown as Parameters<
      typeof turnMadeFromUsage
    >[0];
    expect(turnMadeFromUsage(odd)).toEqual({ turnDurationMs: 5 });
    expect(turnMadeFromMetadata({ contextSize: 4096 })).toEqual({ contextSize: 4096 });
  });
});

describe('turnMadeFromMetadata', () => {
  it('gives nothing for no metadata', () => {
    expect(turnMadeFromMetadata(null)).toBeUndefined();
    expect(turnMadeFromMetadata(undefined)).toBeUndefined();
  });
});
