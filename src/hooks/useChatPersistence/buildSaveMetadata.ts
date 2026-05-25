import type { ThreadMessage, ThreadMessageLike } from '@assistant-ui/react';
import { extractNonTextContentParts, extractReasoningText } from '../../utils/messages';
import type { ChatMessageMetadata } from '../../services/clients/chat';
import type { GglibMessageCustom } from '../../types/messages';

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
  const thinking = extractReasoningText(msg.content);

  const hasContent = parts.length > 0 || thinking !== null;

  // Extract orchestrator run id if present on the message
  const councilRunId = (m.metadata as { custom?: GglibMessageCustom } | undefined)?.custom?.councilRunId;

  if (!hasContent && !councilRunId) return null;

  const meta: ChatMessageMetadata = {};
  if (parts.length > 0) meta.contentParts = parts;
  if (thinking) {
    meta.thinking = thinking;
    if (thinkingDurationSeconds != null) {
      meta.thinkingDurationSeconds = thinkingDurationSeconds;
    }
  }

  // Persist orchestrator run id
  if (councilRunId) {
    meta.councilRunId = councilRunId;
  }

  return meta;
}
