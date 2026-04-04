import type { ThreadMessage, ThreadMessageLike } from '@assistant-ui/react';
import { extractNonTextContentParts } from '../../utils/messages';
import type { ChatMessageMetadata } from '../../services/clients/chat';

/**
 * Build the metadata payload for a DB save or update call.
 *
 * Extracts:
 * - Non-text content parts (tool-call, audio, file, image) → `metadata.contentParts`
 * - Reasoning text from content parts → `metadata.thinking`
 * - Thinking duration (from timing tracker) → `metadata.thinkingDurationSeconds`
 *
 * Returns null when the message contains no structured parts and no reasoning,
 * keeping the metadata column null for plain text messages.
 */
export function buildSaveMetadata(
  m: ThreadMessageLike,
  thinkingDurationSeconds?: number | null,
): ChatMessageMetadata | null {
  const msg = m as unknown as ThreadMessage;
  const parts = extractNonTextContentParts(msg);

  // Extract reasoning text from content parts
  const reasoningChunks: string[] = [];
  for (const part of msg.content) {
    if (
      typeof part === 'object' &&
      part !== null &&
      'type' in part &&
      part.type === 'reasoning' &&
      'text' in part &&
      typeof part.text === 'string'
    ) {
      const trimmed = part.text.trim();
      if (trimmed) reasoningChunks.push(trimmed);
    }
  }
  const thinking = reasoningChunks.length > 0 ? reasoningChunks.join('\n') : null;

  const hasContent = parts.length > 0 || thinking !== null;
  if (!hasContent) return null;

  const meta: ChatMessageMetadata = {};
  if (parts.length > 0) meta.contentParts = parts;
  if (thinking) {
    meta.thinking = thinking;
    if (thinkingDurationSeconds != null) {
      meta.thinkingDurationSeconds = thinkingDurationSeconds;
    }
  }
  return meta;
}
