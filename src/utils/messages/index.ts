/**
 * Message utilities for converting ThreadMessages to text transcripts.
 * 
 * Single source of truth for both rendering and persistence layers.
 * 
 * @module messages
 */

export { wrapThink, isWrappedThink } from './wrapThink';
export { coalesceAdjacentReasoning, type Chunk } from './coalesceReasoning';
export { threadMessageToTranscriptMarkdown } from './threadMessageToTranscriptMarkdown';
