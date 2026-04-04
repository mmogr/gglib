/**
 * Content parts serialization for message persistence.
 *
 * Text is stored in the `content` column as plain markdown.
 * Reasoning is stored in `metadata.thinking`.
 * Non-text parts (tool-call, audio, file, image) are serialized here and
 * stored in `metadata.contentParts` so they survive the DB round-trip.
 *
 * @module contentParts
 */

import type { ThreadMessage, ThreadMessageLike } from '@assistant-ui/react';

// ============================================================================
// Serializable Part Types
// ============================================================================

/** Serializable representation of a tool-call content part. */
export interface SerializableToolCallPart {
  type: 'tool-call';
  toolCallId: string;
  toolName: string;
  args?: Record<string, unknown>;
  argsText?: string;
  result?: unknown;
  isError?: boolean;
}

/** Serializable representation of an audio content part. */
export interface SerializableAudioPart {
  type: 'audio';
  data: string;
  format: string;
}

/** Serializable representation of a file content part. */
export interface SerializableFilePart {
  type: 'file';
  data: string;
  mimeType: string;
}

/** Serializable representation of an image content part. */
export interface SerializableImagePart {
  type: 'image';
  image: string;
}

/** Union of all serializable non-text content parts. */
export type SerializableContentPart =
  | SerializableToolCallPart
  | SerializableAudioPart
  | SerializableFilePart
  | SerializableImagePart;

// ============================================================================
// Extraction (Save Path)
// ============================================================================

/**
 * Extract serializable non-text content parts from a ThreadMessage.
 *
 * Text is stored in the `content` column; reasoning is stored in
 * `metadata.thinking` (see {@link extractReasoningText}). This function
 * captures everything else: tool-call, audio, file, and image parts.
 *
 * @param message - The ThreadMessage from the runtime
 * @returns Array of serializable content parts (empty if none found)
 */
export function extractNonTextContentParts(message: ThreadMessage): SerializableContentPart[] {
  const parts: SerializableContentPart[] = [];

  for (const part of message.content) {
    if (typeof part !== 'object' || part === null || !('type' in part)) {
      continue;
    }

    switch (part.type) {
      case 'tool-call': {
        const tc = part as Record<string, unknown>;
        const tcArgs = tc.args !== undefined ? (tc.args as Record<string, unknown>) : undefined;
        parts.push({
          type: 'tool-call',
          toolCallId: (tc.toolCallId as string) ?? '',
          toolName: (tc.toolName as string) ?? '',
          ...(tcArgs !== undefined && { args: tcArgs }),
          // argsText is computed here at the persistence boundary rather than
          // stored in React state — keeps state minimal and the serialised form
          // always consistent with the current args value.
          ...(tcArgs !== undefined && { argsText: JSON.stringify(tcArgs) }),
          ...(tc.result !== undefined && { result: tc.result }),
          ...(tc.isError !== undefined && { isError: tc.isError as boolean }),
        });
        break;
      }

      case 'audio': {
        const audio = part as Record<string, unknown>;
        // assistant-ui stores audio as { data: string, format: string }
        // or nested as { audio: { data, format } }
        const audioData = (audio.audio as Record<string, unknown>) ?? audio;
        if (typeof audioData.data === 'string' && typeof audioData.format === 'string') {
          parts.push({
            type: 'audio',
            data: audioData.data,
            format: audioData.format,
          });
        }
        break;
      }

      case 'file': {
        const file = part as Record<string, unknown>;
        if (typeof file.data === 'string' && typeof file.mimeType === 'string') {
          parts.push({
            type: 'file',
            data: file.data,
            mimeType: file.mimeType,
          });
        }
        break;
      }

      case 'image': {
        const img = part as Record<string, unknown>;
        if (typeof img.image === 'string') {
          parts.push({
            type: 'image',
            image: img.image,
          });
        }
        break;
      }

      // text, reasoning, source, data, boundary — handled elsewhere or not persisted
      default:
        break;
    }
  }

  return parts;
}

/**
 * Extract concatenated reasoning text from a message's content parts.
 *
 * Iterates the parts array once, collecting trimmed text from every
 * `{ type: 'reasoning', text: string }` entry and joining them with
 * newlines.  Returns `null` when no reasoning is present.
 */
export function extractReasoningText(contentParts: ReadonlyArray<unknown>): string | null {
  const chunks: string[] = [];
  for (const part of contentParts) {
    if (
      typeof part === 'object' &&
      part !== null &&
      'type' in part &&
      (part as Record<string, unknown>).type === 'reasoning' &&
      'text' in part &&
      typeof (part as Record<string, unknown>).text === 'string'
    ) {
      const trimmed = ((part as Record<string, unknown>).text as string).trim();
      if (trimmed) chunks.push(trimmed);
    }
  }
  return chunks.length > 0 ? chunks.join('\n') : null;
}

/**
 * Check whether a message has non-text content that requires structured persistence.
 *
 * @param message - The ThreadMessage from the runtime
 * @returns true if the message contains tool-call, audio, file, or image parts
 */
export function hasNonTextContent(message: ThreadMessage): boolean {
  return message.content.some((part) => {
    if (typeof part !== 'object' || part === null || !('type' in part)) return false;
    return part.type === 'tool-call' || part.type === 'audio' || part.type === 'file' || part.type === 'image';
  });
}

// ============================================================================
// Reconstruction (Load Path)
// ============================================================================

/**
 * Reconstruct ThreadMessageLike content from stored text and structured parts.
 *
 * When `contentParts` are available from metadata, builds a proper content
 * array combining the markdown text (as a text part) with the stored
 * structured parts (tool-call, audio, file, image).
 *
 * When no contentParts are stored (backward compat), returns the text string.
 *
 * @param text - The markdown text stored in the `content` column
 * @param contentParts - Structured parts from `metadata.contentParts` (if any)
 * @returns Content suitable for ThreadMessageLike
 */
export function reconstructContent(
  text: string,
  contentParts?: SerializableContentPart[] | null,
): ThreadMessageLike['content'] {
  if (!contentParts || contentParts.length === 0) {
    return text;
  }

  // Build a content array: text part first (if non-empty), then structured parts
  const parts: Array<Record<string, unknown>> = [];

  if (text.trim()) {
    parts.push({ type: 'text' as const, text });
  }

  for (const cp of contentParts) {
    switch (cp.type) {
      case 'tool-call':
        parts.push({
          type: 'tool-call' as const,
          toolCallId: cp.toolCallId,
          toolName: cp.toolName,
          ...(cp.args !== undefined && { args: cp.args }),
          ...(cp.argsText !== undefined && { argsText: cp.argsText }),
          ...(cp.result !== undefined && { result: cp.result }),
          ...(cp.isError !== undefined && { isError: cp.isError }),
        });
        break;

      case 'audio':
        parts.push({
          type: 'audio' as const,
          audio: { data: cp.data, format: cp.format },
        } as any);
        break;

      case 'file':
        parts.push({
          type: 'file' as const,
          data: cp.data,
          mimeType: cp.mimeType,
        } as any);
        break;

      case 'image':
        parts.push({
          type: 'image' as const,
          image: cp.image,
        } as any);
        break;
    }
  }

  // If we end up with no parts at all, return empty text (shouldn't happen)
  return parts.length > 0 ? (parts as unknown as ThreadMessageLike['content']) : text;
}
