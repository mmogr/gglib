/**
 * Convert ThreadMessage content to markdown transcript.
 * 
 * This is the single source of truth for converting message parts to text,
 * used by both rendering (UI) and persistence (database) layers.
 * 
 * @module threadMessageToTranscriptMarkdown
 */

import type { ThreadMessage } from '@assistant-ui/react';
import { wrapThink, isWrappedThink } from './wrapThink';
import { coalesceAdjacentReasoning, type Chunk } from './coalesceReasoning';
import { appLogger } from '../../services/platform';

// Track unknown part types to avoid log spam during streaming
const warnedUnknownTypes = new Set<string>();

/**
 * Options for threadMessageToTranscriptMarkdown conversion.
 */
export interface TranscriptOptions {
  /**
   * Optional callback to get the duration for a specific reasoning segment.
   * Called once per reasoning block, with segmentIndex matching the order
   * of appearance in the transcript (0-based).
   * 
   * @param messageId - The message ID
   * @param segmentIndex - The index of this reasoning segment (0-based)
   * @returns Duration in seconds, or null if not available
   */
  getDurationForSegment?: (messageId: string, segmentIndex: number) => number | null;
}

/**
 * Convert a ThreadMessage to markdown transcript text.
 * 
 * Handles:
 * - `string` parts (legacy) → pass through unchanged
 * - `{type: 'text'}` parts → extract .text property
 * - `{type: 'reasoning'}` parts → wrap in <think> tags, coalesce adjacent
 * 
 * Skips:
 * - `{type: 'tool-call'}` parts → rendered separately by ToolUsageBadge UI
 * - `{type: 'image'}`, `{type: 'file'}`, etc. → not text-based
 * - Unknown part types → skipped with dev warning (once per type)
 * 
 * Processing:
 * 1. Extract text/reasoning chunks from parts
 * 2. Coalesce adjacent reasoning chunks (no cross-boundary merge)
 * 3. Wrap reasoning in <think> tags (if not already wrapped)
 * 4. Optionally add duration attribute to <think> tags if callback provided
 * 5. Trim each chunk, filter empty, join with '\n\n', trim final output
 * 
 * @param message - The ThreadMessage to convert
 * @param options - Optional configuration for duration injection
 * @returns Markdown transcript text ready for display/persistence
 */
export function threadMessageToTranscriptMarkdown(
  message: ThreadMessage,
  options?: TranscriptOptions
): string {
  const chunks: Chunk[] = [];

  for (const part of message.content) {
    // Handle legacy string parts
    if (typeof part === 'string') {
      const trimmed = (part as string).trim();
      if (trimmed) {
        chunks.push({ type: 'text', content: trimmed });
      }
      continue;
    }

    // Handle object parts by type
    if (typeof part === 'object' && part !== null && 'type' in part) {
      const partType = part.type;

      // Text parts
      if (partType === 'text' && 'text' in part && typeof part.text === 'string') {
        const trimmed = part.text.trim();
        if (trimmed) {
          chunks.push({ type: 'text', content: trimmed });
        }
        continue;
      }

      // Reasoning parts
      if (partType === 'reasoning' && 'text' in part && typeof part.text === 'string') {
        const trimmed = part.text.trim();
        if (trimmed) {
          chunks.push({ type: 'reasoning', content: trimmed });
        }
        continue;
      }

      // Tool-call parts - emit boundary marker (skipped from transcript, rendered separately)
      // Tool results are stored within the tool-call part itself (result property)
      // These mark boundaries between reasoning segments
      if (partType === 'tool-call') {
        chunks.push({ type: 'boundary' });
        continue;
      }

      // Known non-text parts - skip silently
      if (
        partType === 'image' ||
        partType === 'file' ||
        partType === 'audio' ||
        partType === 'source' ||
        partType === 'data'
      ) {
        continue;
      }

      // Unknown part type - warn once
      if (!warnedUnknownTypes.has(partType)) {
        appLogger.warn('util.format', 'Unknown part type', { partType });
        warnedUnknownTypes.add(partType);
      }
    }
  }

  // Coalesce adjacent reasoning chunks
  const coalesced = coalesceAdjacentReasoning(chunks);

  // Convert chunks to markdown, tracking reasoning segment index
  let reasoningSegmentIndex = 0;
  const markdownChunks = coalesced.map((chunk) => {
    if (chunk.type === 'reasoning') {
      // Wrap reasoning in <think> tags if not already wrapped
      if (isWrappedThink(chunk.content)) {
        return chunk.content.trim();
      }
      
      // Get duration for this segment if callback provided
      const duration = options?.getDurationForSegment?.(message.id, reasoningSegmentIndex);
      reasoningSegmentIndex++; // Increment for next reasoning segment
      
      return wrapThink(chunk.content, duration);
    }
    if (chunk.type === 'text') {
      return chunk.content.trim();
    }
    // Boundary chunks are already filtered by coalesceAdjacentReasoning
    return '';
  });

  // Join chunks, filter empty, and trim final output
  return markdownChunks
    .filter((chunk) => chunk.length > 0)
    .join('\n\n')
    .trim();
}
