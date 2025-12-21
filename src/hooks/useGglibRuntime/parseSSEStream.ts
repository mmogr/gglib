/**
 * SSE (Server-Sent Events) stream parser for OpenAI-compatible chat completions.
 *
 * Handles the streaming format with `data:` prefixed lines and extracts
 * content deltas, reasoning content, and tool call deltas from the response.
 *
 * @module parseSSEStream
 */

// =============================================================================
// Types
// =============================================================================

/** Streaming delta for tool call function details */
export interface ToolCallFunctionDelta {
  /** Function name (sent in first chunk) */
  name?: string;
  /** Partial arguments JSON string (accumulated across chunks) */
  arguments?: string;
}

/** Streaming delta for a single tool call */
export interface ToolCallDelta {
  /** Index of the tool call (for parallel tool calls) */
  index: number;
  /** Tool call ID (sent in first chunk) */
  id?: string;
  /** Tool type - always "function" (sent in first chunk) */
  type?: string;
  /** Function delta */
  function?: ToolCallFunctionDelta;
}

/** Delta content from SSE stream */
export interface StreamDelta {
  /** Main content delta */
  content: string | null;
  /** Reasoning/thinking content delta (from reasoning models) */
  reasoningContent: string | null;
  /** Tool call deltas (for function calling) */
  toolCalls: ToolCallDelta[] | null;
  /** Finish reason from the chunk (null during streaming, set on final chunk) */
  finishReason: string | null;
}

// =============================================================================
// Parser
// =============================================================================

/** Marker for incomplete JSON that needs buffering */
const INCOMPLETE_JSON_MARKER = Symbol('INCOMPLETE_JSON');

type ParseLineResult = StreamDelta | 'done' | null | { incomplete: typeof INCOMPLETE_JSON_MARKER; data: string };

/**
 * Parse SSE events from a streaming response.
 *
 * Handles OpenAI-compatible streaming format with `data:` prefixed lines.
 * Extracts `content`, `reasoning_content`, and `tool_calls` from delta objects.
 *
 * Features:
 * - Handles nested SSE (backend wraps with `data:`, llama-server also uses `data:`)
 * - Buffers incomplete lines across chunks
 * - Buffers incomplete JSON that spans multiple chunks
 * - Gracefully skips non-JSON lines (comments, keep-alive)
 * - Respects abort signals for cancellation
 *
 * @param reader - ReadableStream reader for the response body
 * @param abortSignal - Optional signal to abort parsing
 * @yields StreamDelta objects containing content, reasoning, tool calls, and finish reason
 *
 * @example
 * ```ts
 * const reader = response.body.getReader();
 * for await (const delta of parseSSEStream(reader, abortSignal)) {
 *   if (delta.content) {
 *     console.log('Content:', delta.content);
 *   }
 *   if (delta.toolCalls) {
 *     console.log('Tool calls:', delta.toolCalls);
 *   }
 * }
 * ```
 */
export async function* parseSSEStream(
  reader: ReadableStreamDefaultReader<Uint8Array>,
  abortSignal?: AbortSignal
): AsyncGenerator<StreamDelta, void, unknown> {
  const decoder = new TextDecoder();
  let buffer = '';
  let pendingIncompleteJson = ''; // Buffer for incomplete JSON

  try {
    while (true) {
      if (abortSignal?.aborted) {
        break;
      }

      const { done, value } = await reader.read();
      if (done) break;

      buffer += decoder.decode(value, { stream: true });

      // Process complete lines
      const lines = buffer.split('\n');
      buffer = lines.pop() || ''; // Keep incomplete line in buffer

      for (const line of lines) {
        // If we have pending incomplete JSON, try combining with this line
        let lineToProcess = line;
        if (pendingIncompleteJson) {
          lineToProcess = pendingIncompleteJson + line;
          pendingIncompleteJson = '';
        }

        const result = parseLine(lineToProcess);
        
        // Check for incomplete JSON marker
        if (result && typeof result === 'object' && 'incomplete' in result) {
          pendingIncompleteJson = result.data;
          continue;
        }
        
        if (result === 'done') {
          return;
        }
        if (result) {
          yield result;
        }
      }
    }

    // Process any remaining buffer content
    if (buffer.trim() || pendingIncompleteJson) {
      const lineToProcess = pendingIncompleteJson + buffer;
      const result = parseLine(lineToProcess);
      if (result && result !== 'done' && !('incomplete' in result)) {
        yield result;
      }
    }
  } finally {
    reader.releaseLock();
  }
}

/**
 * Parse a single SSE line into a StreamDelta or signal completion.
 *
 * @param line - Raw SSE line to parse
 * @returns StreamDelta if valid data, 'done' if stream termination, incomplete marker, or null to skip
 */
function parseLine(line: string): ParseLineResult {
  const trimmed = line.trim();
  if (!trimmed) return null;

  // Handle nested SSE (our backend wraps with data:)
  let dataLine = trimmed;
  if (dataLine.startsWith('data:')) {
    dataLine = dataLine.slice(5).trim();
  }

  // Check for stream termination
  if (dataLine === '[DONE]') {
    return 'done';
  }

  // Handle inner data: prefix from llama-server SSE
  if (dataLine.startsWith('data:')) {
    dataLine = dataLine.slice(5).trim();
    if (dataLine === '[DONE]') {
      return 'done';
    }
  }

  // Skip empty data
  if (!dataLine) return null;

  // Parse JSON chunk and extract content deltas
  try {
    const chunk = JSON.parse(dataLine);
    const choice = chunk.choices?.[0];
    const delta = choice?.delta;
    const finishReason = choice?.finish_reason ?? null;

    // Extract content, reasoning_content, and tool_calls from delta
    const contentDelta = delta?.content ?? null;
    const reasoningDelta = delta?.reasoning_content ?? null;
    const toolCallsDelta: ToolCallDelta[] | null = delta?.tool_calls ?? null;

    // Return if we have any content, tool calls, or finish_reason
    if (contentDelta || reasoningDelta || toolCallsDelta || finishReason) {
      return {
        content: contentDelta,
        reasoningContent: reasoningDelta,
        toolCalls: toolCallsDelta,
        finishReason,
      };
    }

    return null;
  } catch {
    // Check if this looks like truncated JSON (starts with { but doesn't parse)
    if (dataLine.startsWith('{') || dataLine.startsWith('[')) {
      // This is likely truncated JSON - return marker to signal caller to buffer
      return { incomplete: INCOMPLETE_JSON_MARKER, data: dataLine };
    }
    // Skip non-JSON lines (e.g., comments, keep-alive, event: prefixes)
    return null;
  }
}
