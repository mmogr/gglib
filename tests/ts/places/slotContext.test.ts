/**
 * One slot's context, as `gglib proxy dashboard` prints it slot by slot: a
 * slot whose size is not known says nothing, and a slot holding more tokens
 * than its size reads as full, never over 100%.
 */

import { describe, it, expect } from 'vitest';
import { slotContext, slotContextWords } from '../../../src/places/diagnostics/live/slotContext';
import type { SlotSnapshot } from '../../../src/services/transport/types/dashboard';

const slot = (over: Partial<SlotSnapshot>): SlotSnapshot => ({ id: 0, tokens_in_use: 1024, ...over }) as SlotSnapshot;

describe('slotContext', () => {
  it('says nothing for a slot of size 0 or of no known size', () => {
    expect(slotContext(slot({ n_ctx: 0 }))).toBeNull();
    expect(slotContext(slot({ n_ctx: null }))).toBeNull();
  });

  it('gives the tokens of its own size, and the percent', () => {
    expect(slotContextWords(slotContext(slot({ n_ctx: 4096 }))!)).toBe('1,024 of 4,096 tokens · 25%');
  });

  it('reads a slot holding more than its size as full, never over 100%', () => {
    expect(slotContext(slot({ n_ctx: 512 }))?.percent).toBe(100);
  });
});
