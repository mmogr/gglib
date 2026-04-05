/**
 * Convert ThreadMessage content to markdown transcript.
 * 
 * This is the single source of truth for converting message parts to text,
 * used by both rendering (UI) and persistence (database) layers.
 * 
 * @module threadMessageToTranscriptMarkdown
 */

import type { ThreadMessage } from '@assistant-ui/react';
import { appLogger } from '../../services/platform';

// Track unknown part types to avoid log spam during streaming
const warnedUnknownTypes = new Set<string>();

/**
 * Convert a ThreadMessage to markdown transcript text.
 * 
 * Produces the *answer-only* text stored in the DB `content` column.
 * Reasoning parts are persisted separately in `metadata.thinking` by
 * `buildSaveMetadata`, so they are intentionally excluded here.
 * 
 * Handles:
 * - `string` parts (legacy) → pass through unchanged
 * - `{type: 'text'}` parts → extract .text property
 * 
 * Skips:
 * - `{type: 'reasoning'}` parts → persisted in metadata, not in content
 * - `{type: 'tool-call'}` parts → rendered separately by ToolUsageBadge UI
 * - `{type: 'image'}`, `{type: 'file'}`, etc. → not text-based
 * - Unknown part types → skipped with dev warning (once per type)
 * 
 * @param message - The ThreadMessage to convert
 * @returns Markdown transcript text (answer only, no reasoning)
 */
export function threadMessageToTranscriptMarkdown(
  message: ThreadMessage,
): string {
  const textChunks: string[] = [];

  for (const part of message.content) {
    // Handle legacy string parts
    if (typeof part === 'string') {
      const trimmed = (part as string).trim();
      if (trimmed) {
        textChunks.push(trimmed);
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
          textChunks.push(trimmed);
        }
        continue;
      }

      // Reasoning parts — stored in metadata.thinking, skip here
      if (partType === 'reasoning') {
        continue;
      }

      // Tool-call parts — rendered separately, skip
      if (partType === 'tool-call') {
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

  return textChunks
    .filter((chunk) => chunk.length > 0)
    .join('\n\n')
    .trim();
}
