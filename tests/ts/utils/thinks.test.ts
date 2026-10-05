/**
 * Whether a model thinks, asked of this machine's row and of the paired
 * machine's: the one answer the chat page's Thinking switch is shown by.
 */

import { describe, it, expect } from 'vitest';
import { thinks } from '../../../src/utils/thinks';
import { CAPABILITY_FLAGS } from '../../../src/types';
import { guiModel } from '../fixtures/model';

describe('thinks', () => {
  it('answers a local row by its reasoning tag, among its others', () => {
    expect(thinks({ tags: ['reasoning'] })).toBe(true);
    expect(thinks({ tags: ['agent', 'reasoning', 'mtp'] })).toBe(true);
    expect(thinks({ tags: ['agent', 'code'] })).toBe(false);
    expect(thinks({ tags: [] })).toBe(false);
  });

  it('matches the tag in any case, as the daemon does', () => {
    expect(thinks({ tags: ['Reasoning'] })).toBe(true);
    expect(thinks({ tags: ['REASONING'] })).toBe(true);
  });

  it('matches the whole tag, never a part of one', () => {
    expect(thinks({ tags: ['reasoning-lite'] })).toBe(false);
    expect(thinks({ tags: ['non-reasoning'] })).toBe(false);
    expect(thinks({ tags: [' reasoning'] })).toBe(false);
  });

  it('answers a far row by the reasoning capability its machine lists', () => {
    expect(thinks({ capabilities: ['reasoning'] })).toBe(true);
    expect(thinks({ capabilities: ['vision', 'reasoning'] })).toBe(true);
  });

  it('answers no for a far row with other capabilities, or none: an older machine lists none', () => {
    expect(thinks({ capabilities: ['vision'] })).toBe(false);
    expect(thinks({ capabilities: ['Reasoning'] })).toBe(false);
    expect(thinks({ capabilities: [] })).toBe(false);
    expect(thinks({})).toBe(false);
  });

  it('never reads a local row by its capability bit: the bit alone is not thinking, and the tag needs no bit', () => {
    const bitOnly = guiModel({ tags: [], capabilities: CAPABILITY_FLAGS.supportsReasoning });
    const tagOnly = guiModel({ tags: ['reasoning'], capabilities: 0 });
    expect(thinks(bitOnly)).toBe(false);
    expect(thinks(tagOnly)).toBe(true);
  });
});
