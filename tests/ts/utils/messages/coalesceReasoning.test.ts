/**
 * Tests for coalesceAdjacentReasoning utility.
 * 
 * Tests coalescing of adjacent reasoning chunks in message content.
 */

import { describe, it, expect } from 'vitest';
import { coalesceAdjacentReasoning, type Chunk } from '../../../../src/utils/messages/coalesceReasoning';

describe('coalesceAdjacentReasoning', () => {
  it('returns empty array for empty input', () => {
    expect(coalesceAdjacentReasoning([])).toEqual([]);
  });

  it('returns single chunk unchanged', () => {
    const chunks: Chunk[] = [{ type: 'text', content: 'Hello' }];
    expect(coalesceAdjacentReasoning(chunks)).toEqual(chunks);
  });

  it('merges two adjacent reasoning chunks', () => {
    const chunks: Chunk[] = [
      { type: 'reasoning', content: 'First thought' },
      { type: 'reasoning', content: 'Second thought' },
    ];
    expect(coalesceAdjacentReasoning(chunks)).toEqual([
      { type: 'reasoning', content: 'First thought\n\nSecond thought' },
    ]);
  });

  it('merges multiple adjacent reasoning chunks', () => {
    const chunks: Chunk[] = [
      { type: 'reasoning', content: 'One' },
      { type: 'reasoning', content: 'Two' },
      { type: 'reasoning', content: 'Three' },
    ];
    expect(coalesceAdjacentReasoning(chunks)).toEqual([
      { type: 'reasoning', content: 'One\n\nTwo\n\nThree' },
    ]);
  });

  it('does not merge reasoning across text boundary', () => {
    const chunks: Chunk[] = [
      { type: 'reasoning', content: 'First thought' },
      { type: 'text', content: 'Some text' },
      { type: 'reasoning', content: 'Second thought' },
    ];
    expect(coalesceAdjacentReasoning(chunks)).toEqual([
      { type: 'reasoning', content: 'First thought' },
      { type: 'text', content: 'Some text' },
      { type: 'reasoning', content: 'Second thought' },
    ]);
  });

  it('handles reasoning at start and end with text in middle', () => {
    const chunks: Chunk[] = [
      { type: 'reasoning', content: 'Think 1' },
      { type: 'reasoning', content: 'Think 2' },
      { type: 'text', content: 'Answer' },
      { type: 'reasoning', content: 'Think 3' },
      { type: 'reasoning', content: 'Think 4' },
    ];
    expect(coalesceAdjacentReasoning(chunks)).toEqual([
      { type: 'reasoning', content: 'Think 1\n\nThink 2' },
      { type: 'text', content: 'Answer' },
      { type: 'reasoning', content: 'Think 3\n\nThink 4' },
    ]);
  });

  it('handles only text chunks', () => {
    const chunks: Chunk[] = [
      { type: 'text', content: 'One' },
      { type: 'text', content: 'Two' },
      { type: 'text', content: 'Three' },
    ];
    expect(coalesceAdjacentReasoning(chunks)).toEqual(chunks);
  });

  it('handles only reasoning chunks', () => {
    const chunks: Chunk[] = [
      { type: 'reasoning', content: 'One' },
      { type: 'reasoning', content: 'Two' },
      { type: 'reasoning', content: 'Three' },
    ];
    expect(coalesceAdjacentReasoning(chunks)).toEqual([
      { type: 'reasoning', content: 'One\n\nTwo\n\nThree' },
    ]);
  });

  it('preserves text chunks as-is', () => {
    const chunks: Chunk[] = [
      { type: 'text', content: 'Start' },
      { type: 'reasoning', content: 'Think' },
      { type: 'text', content: 'End' },
    ];
    expect(coalesceAdjacentReasoning(chunks)).toEqual(chunks);
  });

  it('handles complex interleaving', () => {
    const chunks: Chunk[] = [
      { type: 'reasoning', content: 'R1' },
      { type: 'text', content: 'T1' },
      { type: 'reasoning', content: 'R2' },
      { type: 'reasoning', content: 'R3' },
      { type: 'text', content: 'T2' },
      { type: 'text', content: 'T3' },
      { type: 'reasoning', content: 'R4' },
    ];
    expect(coalesceAdjacentReasoning(chunks)).toEqual([
      { type: 'reasoning', content: 'R1' },
      { type: 'text', content: 'T1' },
      { type: 'reasoning', content: 'R2\n\nR3' },
      { type: 'text', content: 'T2' },
      { type: 'text', content: 'T3' },
      { type: 'reasoning', content: 'R4' },
    ]);
  });

  // Boundary chunk tests
  it('does not merge reasoning across boundary marker', () => {
    const chunks: Chunk[] = [
      { type: 'reasoning', content: 'First thought' },
      { type: 'boundary' },
      { type: 'reasoning', content: 'Second thought' },
    ];
    expect(coalesceAdjacentReasoning(chunks)).toEqual([
      { type: 'reasoning', content: 'First thought' },
      { type: 'reasoning', content: 'Second thought' },
    ]);
  });

  it('filters out boundary markers after use', () => {
    const chunks: Chunk[] = [
      { type: 'text', content: 'Start' },
      { type: 'boundary' },
      { type: 'text', content: 'End' },
    ];
    expect(coalesceAdjacentReasoning(chunks)).toEqual([
      { type: 'text', content: 'Start' },
      { type: 'text', content: 'End' },
    ]);
  });

  it('handles multiple boundaries between reasoning', () => {
    const chunks: Chunk[] = [
      { type: 'reasoning', content: 'R1' },
      { type: 'boundary' },
      { type: 'boundary' },
      { type: 'reasoning', content: 'R2' },
    ];
    expect(coalesceAdjacentReasoning(chunks)).toEqual([
      { type: 'reasoning', content: 'R1' },
      { type: 'reasoning', content: 'R2' },
    ]);
  });

  it('handles complex with boundaries', () => {
    const chunks: Chunk[] = [
      { type: 'reasoning', content: 'Think 1' },
      { type: 'reasoning', content: 'Think 2' },
      { type: 'boundary' },  // Tool call
      { type: 'reasoning', content: 'Think 3' },
      { type: 'text', content: 'Answer' },
      { type: 'boundary' },  // Another tool call
      { type: 'reasoning', content: 'Think 4' },
    ];
    expect(coalesceAdjacentReasoning(chunks)).toEqual([
      { type: 'reasoning', content: 'Think 1\n\nThink 2' },
      { type: 'reasoning', content: 'Think 3' },
      { type: 'text', content: 'Answer' },
      { type: 'reasoning', content: 'Think 4' },
    ]);
  });
});
