/**
 * Content parts serialization for message persistence.
 *
 * Text is stored in the `content` column as plain markdown.
 * Reasoning is stored in `metadata.thinking`.
 * Tool calls are stored in `metadata.contentParts` and rebuilt here, so they
 * survive the DB round-trip. An image is not a content part: a message names
 * it by id, in the row's `images`.
 *
 * @module contentParts
 */

import type { ThreadMessageLike } from '@assistant-ui/react';

// ============================================================================
// Serializable Part Type
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

// ============================================================================
// Extraction
// ============================================================================

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

// ============================================================================
// Reconstruction (Load Path)
// ============================================================================

/**
 * Reconstruct ThreadMessageLike content from stored text and tool calls.
 *
 * When `contentParts` are available from metadata, builds a content array:
 * the markdown text (as a text part) and then each stored tool call. A stored
 * part of any other type is left out.
 *
 * When no contentParts are stored (backward compat), returns the text string.
 *
 * @param text - The markdown text stored in the `content` column
 * @param contentParts - Tool calls from `metadata.contentParts` (if any)
 * @returns Content suitable for ThreadMessageLike
 */
export function reconstructContent(
  text: string,
  contentParts?: SerializableToolCallPart[] | null,
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
    // The column is free JSON: only what is known to be a tool call is rebuilt.
    if (cp.type !== 'tool-call') continue;
    parts.push({
      type: 'tool-call' as const,
      toolCallId: cp.toolCallId,
      toolName: cp.toolName,
      ...(cp.args !== undefined && { args: cp.args }),
      ...(cp.argsText !== undefined && { argsText: cp.argsText }),
      ...(cp.result !== undefined && { result: cp.result }),
      ...(cp.isError !== undefined && { isError: cp.isError }),
    });
  }

  // If we end up with no parts at all, return empty text (shouldn't happen)
  return parts.length > 0 ? (parts as unknown as ThreadMessageLike['content']) : text;
}
