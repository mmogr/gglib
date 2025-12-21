/**
 * Utilities for coalescing adjacent reasoning chunks in message content.
 * 
 * @module coalesceReasoning
 */

/**
 * A chunk of message content - reasoning, text, or boundary marker.
 * 
 * Boundary chunks mark skipped parts (tool-call, tool-result, tool-output) that
 * should prevent reasoning coalescing but aren't rendered in the transcript.
 */
export type Chunk = 
  | { type: 'reasoning'; content: string }
  | { type: 'text'; content: string }
  | { type: 'boundary' };

/**
 * Coalesce adjacent reasoning chunks into single chunks.
 * 
 * Adjacent reasoning chunks are merged together with '\n\n' separator.
 * Does NOT coalesce across text or other non-reasoning boundaries.
 * This maintains clean "thinking → action → thinking" flow.
 * 
 * Examples:
 * - [reasoning, reasoning, text] → [reasoning (merged), text]
 * - [reasoning, text, reasoning] → [reasoning, text, reasoning] (no merge)
 * - [text, reasoning, reasoning] → [text, reasoning (merged)]
 * 
 * @param chunks - Array of chunks to process
 * @returns Array with adjacent reasoning chunks merged
 */
export function coalesceAdjacentReasoning(chunks: Chunk[]): Chunk[] {
  if (chunks.length === 0) return [];

  const result: Chunk[] = [];
  let currentReasoning: string | null = null;

  for (const chunk of chunks) {
    if (chunk.type === 'reasoning') {
      // Accumulate reasoning chunks
      if (currentReasoning === null) {
        currentReasoning = chunk.content;
      } else {
        currentReasoning += '\n\n' + chunk.content;
      }
    } else {
      // Non-reasoning chunk (text or boundary): flush any accumulated reasoning first
      if (currentReasoning !== null) {
        result.push({ type: 'reasoning', content: currentReasoning });
        currentReasoning = null;
      }
      // Keep text chunks, skip boundary markers (they've served their purpose)
      if (chunk.type === 'text') {
        result.push(chunk);
      }
      // Boundary chunks are discarded after breaking coalescing
    }
  }

  // Flush any remaining reasoning
  if (currentReasoning !== null) {
    result.push({ type: 'reasoning', content: currentReasoning });
  }

  return result;
}
