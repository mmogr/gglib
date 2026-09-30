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
});

describe('turnMadeFromMetadata', () => {
  it('gives nothing for no metadata', () => {
    expect(turnMadeFromMetadata(null)).toBeUndefined();
    expect(turnMadeFromMetadata(undefined)).toBeUndefined();
  });
});
