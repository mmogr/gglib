import type { ThreadMessage, ThreadMessageLike } from '@assistant-ui/react';
import { extractNonTextContentParts } from '../../utils/messages';
import type { ChatMessageMetadata } from '../../services/clients/chat';

/**
 * Build the metadata payload for a DB save or update call.
 *
 * Extracts non-text content parts (tool-call, audio, file, image) from a
 * runtime message and packages them as `metadata.contentParts` so they can
 * be restored by buildLoadedMessage on next hydration.
 *
 * Returns null when the message contains no structured parts, keeping the
 * metadata column null for plain text messages.
 */
export function buildSaveMetadata(m: ThreadMessageLike): ChatMessageMetadata | null {
  const parts = extractNonTextContentParts(m as unknown as ThreadMessage);
  return parts.length > 0 ? { contentParts: parts } : null;
}
