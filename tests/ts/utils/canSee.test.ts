/**
 * Whether a model reads images, asked of this machine's row and of the
 * paired machine's.
 */

import { describe, it, expect } from 'vitest';
import { canSee } from '../../../src/utils/canSee';

describe('canSee', () => {
  it('answers a local row by its imageInput', () => {
    expect(canSee({ imageInput: true })).toBe(true);
    expect(canSee({ imageInput: false })).toBe(false);
  });

  it('answers a far row by the vision capability its machine lists', () => {
    expect(canSee({ capabilities: ['vision'] })).toBe(true);
    expect(canSee({ capabilities: ['embeddings', 'vision'] })).toBe(true);
  });

  it('answers no for a far row with other capabilities, or none', () => {
    expect(canSee({ capabilities: ['embeddings'] })).toBe(false);
    expect(canSee({ capabilities: [] })).toBe(false);
    expect(canSee({})).toBe(false);
  });

  it('reads a local row by imageInput, whatever its own capabilities field holds', () => {
    // A local row's `capabilities` is a bitset of template quirks, not a list.
    const sees = { imageInput: true, capabilities: 0 };
    const blind = { imageInput: false, capabilities: 3 };
    expect(canSee(sees)).toBe(true);
    expect(canSee(blind)).toBe(false);
  });
});
