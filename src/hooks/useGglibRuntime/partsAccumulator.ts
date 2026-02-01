/**
 * Parts accumulator for assistant message content.
 *
 * Manages all content parts for a single assistant message: reasoning blocks,
 * text content, tool calls, and tool results. Provides a snapshot() method
 * that returns the current state as an array of message parts.
 *
 * This ensures that yielding content during streaming and tool execution
 * always includes ALL parts accumulated so far, preventing overwrites.
 *
 * @module partsAccumulator
 */

import type {
  MessagePart,
  ToolCallPart,
  ReasoningPart,
  TextPart,
} from '../../types/messages';
import { appLogger } from '../../services/platform';

// =============================================================================
// Types (re-exported from messages for convenience)
// =============================================================================

export type { MessagePart, ReasoningPart, TextPart, ToolCallPart } from '../../types/messages';

interface ReasoningBlock {
  id: string;
  text: string;
}

// =============================================================================
// PartsAccumulator
// =============================================================================

/**
 * Accumulator for all message parts in an assistant message.
 *
 * Maintains stable ordering: reasoning blocks, text, tool calls, tool results.
 * Call snapshot() to get the current array of parts for yielding.
 */
export class PartsAccumulator {
  /** Main text content (non-reasoning) */
  text = '';

  /** Completed and in-progress reasoning blocks */
  reasoningBlocks: ReasoningBlock[] = [];

  /** Tool call parts indexed by call ID (includes results after execution) */
  toolCalls: Map<string, ToolCallPart> = new Map();

  /**
   * Get current snapshot of all parts.
   * Returns array in stable order: reasoning, text, tool-calls, tool-results.
   */
  snapshot(): MessagePart[] {
    const parts: MessagePart[] = [];

    // Add reasoning blocks (in order)
    for (const block of this.reasoningBlocks) {
      parts.push({ type: 'reasoning', text: block.text } as ReasoningPart);
    }

    // Add text content if present
    if (this.text) {
      parts.push({ type: 'text', text: this.text } as TextPart);
    }

    // Add tool calls (includes results if executed)
    parts.push(...this.toolCalls.values());

    // Logging to verify reasoning parts exist at ingestion point
    if (parts.some(p => typeof p === 'object' && 'type' in p && p.type === 'reasoning')) {
      appLogger.debug('hook.runtime', 'PartsAccumulator snapshot with reasoning', { partCount: parts.length });
    }

    return parts;
  }

  /**
   * Start a new reasoning block.
   * @param id - Unique identifier for this block
   */
  startReasoningBlock(id: string): void {
    this.reasoningBlocks.push({ id, text: '' });
  }

  /**
   * Append text to the current reasoning block.
   * @param content - Text to append
   */
  appendToCurrentReasoning(content: string): void {
    const current = this.reasoningBlocks[this.reasoningBlocks.length - 1];
    if (current) {
      current.text += content;
    }
  }

  /**
   * Check if currently accumulating a reasoning block.
   */
  hasActiveReasoning(): boolean {
    return this.reasoningBlocks.length > 0;
  }

  /**
   * Append to main text content.
   * @param content - Text to append
   */
  appendText(content: string): void {
    this.text += content;
  }

  /**
   * Add or update a tool call part.
   * @param toolCall - Tool call part to add/update
   */
  setToolCall(toolCall: ToolCallPart): void {
    if (!toolCall.toolCallId) return;
    this.toolCalls.set(toolCall.toolCallId, toolCall);
  }

  /**
   * Update a tool call with its result after execution.
   * @param toolCallId - ID of the tool call to update
   * @param result - Result from tool execution
   * @param isError - Whether the tool execution failed
   */
  setToolResult(toolCallId: string, result: unknown, isError: boolean = false): void {
    const toolCall = this.toolCalls.get(toolCallId);
    if (toolCall) {
      // Create new object since properties are readonly
      this.toolCalls.set(toolCallId, { ...toolCall, result, isError });
    }
  }
}
