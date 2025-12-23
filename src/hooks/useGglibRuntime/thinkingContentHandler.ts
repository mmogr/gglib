/**
 * Thinking content handler for streaming chat responses.
 *
 * Manages reasoning/thinking phases from reasoning models by updating a
 * PartsAccumulator. Handles both:
 * - `reasoning_content` field from the SSE delta (DeepSeek R1 native format)
 * - Inline `<think>...</think>` tags in content (parsed during streaming)
 *
 * Multiple reasoning blocks are preserved within the same assistant message.
 *
 * @module useThinkingContent
 */

import { parseStreamingThinkingContent } from '../../utils/thinkingParser';
import { PartsAccumulator } from './partsAccumulator';

// =============================================================================
// Types
// =============================================================================

/** Thinking content handler interface */
export interface ThinkingContentHandler {
  /**
   * Handle a reasoning_content delta from SSE stream.
   * Updates the accumulator's current reasoning block.
   * @param content - The reasoning content delta
   * @param acc - The parts accumulator to update
   */
  handleReasoningDelta(content: string, acc?: PartsAccumulator): void;

  /**
   * Handle main content delta that may contain inline <think> tags.
   * Updates the accumulator's text and/or reasoning.
   * @param content - The main content delta
   * @param accumulated - The total accumulated main content so far
   * @param acc - The parts accumulator to update
   */
  handleReasoningDelta(content: string, acc: PartsAccumulator): void;

  /**
   * Handle main content delta that may contain inline <think> tags.
   * Updates the accumulator's text and/or reasoning.
   * @param content - The main content delta
   * @param accumulated - The total accumulated main content so far
   * @param acc - The parts accumulator to update
   */
  handleContentDelta(content: string, accumulated: string, acc: PartsAccumulator): void;

  /**
   * Signal that main (non-thinking) content has started.
   * Finalizes the current reasoning block.
   */
  markMainContentStarted(): void;

  /**
   * Check if currently accumulating reasoning content.
   */
  isThinking(): boolean;
}

// =============================================================================
// Factory
// =============================================================================

/**
 * Create a new thinking content handler.
 *
 * Works with a PartsAccumulator to manage reasoning and text parts.
 * Each reasoning phase creates a new reasoning block in the accumulator.
 *
 * @returns A new ThinkingContentHandler instance
 *
 * @example
 * ```ts
 * const acc = new PartsAccumulator();
 * const thinkingHandler = createThinkingContentHandler();
 *
 * for await (const delta of parseSSEStream(reader)) {
 *   if (delta.reasoningContent) {
 *     thinkingHandler.handleReasoningDelta(delta.reasoningContent, acc);
 *   }
 *   if (delta.content) {
 *     mainContent += delta.content;
 *     thinkingHandler.handleContentDelta(delta.content, mainContent, acc);
 *     if (!hasReceivedMainContent && thinkingHandler.isThinking()) {
 *       thinkingHandler.markMainContentStarted();
 *       hasReceivedMainContent = true;
 *     }
 *   }
 *
 *   yield { content: acc.snapshot() };
 * }
 * ```
 */
export function createThinkingContentHandler(): ThinkingContentHandler {
  // Tracks whether we've started a reasoning block
  let reasoningStarted = false;

  // Tracks whether we've finalized reasoning and moved to main content
  let reasoningFinalized = false;

  // Inline thinking state
  let lastInlineThinking = '';

  function handleReasoningDelta(content: string, acc: PartsAccumulator): void {
    if (!reasoningStarted) {
      // Start a new reasoning block
      const blockId = `reasoning-${Date.now()}`;
      acc.startReasoningBlock(blockId);
      reasoningStarted = true;
    }

    // Append to current reasoning block
    acc.appendToCurrentReasoning(content);
  }

  function handleContentDelta(_content: string, accumulated: string, acc: PartsAccumulator): void {
    // Check for inline thinking in accumulated content
    const parsed = parseStreamingThinkingContent(accumulated);
    
    // If we find inline <think> tags, treat that as reasoning
    if (parsed.thinking && parsed.thinking !== lastInlineThinking) {
      if (!reasoningStarted) {
        // Start new reasoning block for inline thinking
        const blockId = `reasoning-inline-${Date.now()}`;
        acc.startReasoningBlock(blockId);
        reasoningStarted = true;
      }
      
      // Update reasoning block with full inline thinking content
      const current = acc.reasoningBlocks[acc.reasoningBlocks.length - 1];
      if (current) {
        current.text = parsed.thinking;
      }
      lastInlineThinking = parsed.thinking;

      if (parsed.isThinkingComplete) {
        reasoningFinalized = true;
      }
    }

    // Update main text (without think tags)
    acc.text = parsed.content;
  }

  function markMainContentStarted(): void {
    reasoningFinalized = true;
  }

  function isThinking(): boolean {
    return reasoningStarted && !reasoningFinalized;
  }

  return {
    handleReasoningDelta,
    handleContentDelta,
    markMainContentStarted,
    isThinking,
  };
}
