/**
 * Thinking content handler for streaming chat responses.
 *
 * Manages timing and content accumulation for reasoning/thinking phases
 * from reasoning models. Handles both:
 * - `reasoning_content` field from the SSE delta (DeepSeek R1 native format)
 * - Inline `<think>...</think>` tags in content (parsed during streaming)
 *
 * @module useThinkingContent
 */

import {
  embedThinkingContent,
  parseStreamingThinkingContent,
} from '../../utils/thinkingParser';

// =============================================================================
// Types
// =============================================================================

/** Current thinking state during streaming */
export interface ThinkingState {
  /** Whether we're currently in a thinking phase */
  isThinking: boolean;
  /** Accumulated thinking content (from reasoning_content field) */
  thinkingContent: string;
  /** Start time of thinking phase (ms since epoch) */
  startedAt: number | null;
  /** End time of thinking phase (ms since epoch), null if still thinking */
  endedAt: number | null;
}

/** Inline thinking state for <think> tag parsing */
export interface InlineThinkingState {
  /** Start time of inline thinking (ms since epoch) */
  startedAt: number | null;
  /** End time of inline thinking (ms since epoch) */
  endedAt: number | null;
}

/** Combined state for building display content */
export interface ThinkingDisplayState {
  /** The accumulated thinking/reasoning content */
  thinkingContent: string;
  /** The main content (non-thinking) */
  mainContent: string;
  /** Thinking duration in seconds, or null if not applicable */
  durationSeconds: number | null;
}

/** Thinking content handler interface */
export interface ThinkingContentHandler {
  /**
   * Handle a reasoning_content delta from SSE stream.
   * @param content - The reasoning content delta
   */
  handleReasoningDelta(content: string): void;

  /**
   * Handle main content delta that may contain inline <think> tags.
   * @param content - The main content delta
   * @param accumulated - The total accumulated main content so far
   */
  handleContentDelta(content: string, accumulated: string): void;

  /**
   * Signal that main (non-thinking) content has started.
   * Used to mark the end of the thinking phase timing.
   */
  markMainContentStarted(): void;

  /**
   * Build display content with thinking embedded.
   * @param mainContent - The accumulated main content
   * @returns Formatted content with thinking tags and duration
   */
  buildDisplayContent(mainContent: string): string;

  /**
   * Build final content after streaming is complete.
   * @param mainContent - The final accumulated main content
   * @returns Formatted content with thinking tags and final duration
   */
  buildFinalContent(mainContent: string): string;

  /**
   * Get the current thinking state.
   */
  getState(): ThinkingState;

  /**
   * Reset the handler to initial state.
   */
  reset(): void;
}

// =============================================================================
// Factory
// =============================================================================

/**
 * Create a new thinking content handler.
 *
 * Manages two sources of thinking content:
 * 1. `reasoning_content` SSE delta field (from models like DeepSeek R1)
 * 2. Inline `<think>...</think>` tags in the main content
 *
 * @returns A new ThinkingContentHandler instance
 *
 * @example
 * ```ts
 * const thinkingHandler = createThinkingContentHandler();
 *
 * for await (const delta of parseSSEStream(reader)) {
 *   if (delta.reasoningContent) {
 *     thinkingHandler.handleReasoningDelta(delta.reasoningContent);
 *   }
 *   if (delta.content) {
 *     mainContent += delta.content;
 *     thinkingHandler.handleContentDelta(delta.content, mainContent);
 *     if (!hasReceivedMainContent && thinkingHandler.getState().thinkingContent) {
 *       thinkingHandler.markMainContentStarted();
 *       hasReceivedMainContent = true;
 *     }
 *   }
 *
 *   const displayContent = thinkingHandler.buildDisplayContent(mainContent);
 *   yield { content: [{ type: 'text', text: displayContent }] };
 * }
 *
 * const finalContent = thinkingHandler.buildFinalContent(mainContent);
 * ```
 */
export function createThinkingContentHandler(): ThinkingContentHandler {
  // State for reasoning_content field
  let thinkingContent = '';
  let thinkingStartedAt: number | null = null;
  let thinkingEndedAt: number | null = null;

  // State for inline <think> tags
  let inlineStartedAt: number | null = null;
  let inlineEndedAt: number | null = null;

  function handleReasoningDelta(content: string): void {
    if (!thinkingStartedAt) {
      thinkingStartedAt = Date.now();
    }
    thinkingContent += content;
  }

  function handleContentDelta(_content: string, accumulated: string): void {
    // Check for inline thinking in accumulated content
    const parsed = parseStreamingThinkingContent(accumulated);
    if (parsed.thinking && inlineStartedAt === null) {
      inlineStartedAt = Date.now();
    }
    if (parsed.isThinkingComplete && inlineEndedAt === null && parsed.thinking) {
      inlineEndedAt = Date.now();
    }
  }

  function markMainContentStarted(): void {
    if (thinkingContent && !thinkingEndedAt) {
      thinkingEndedAt = Date.now();
    }
  }

  function buildDisplayContent(mainContent: string): string {
    // Case 1: We have reasoning_content from the SSE delta
    if (thinkingContent) {
      const currentEndTime = thinkingEndedAt ?? Date.now();
      const durationSeconds = (currentEndTime - (thinkingStartedAt ?? currentEndTime)) / 1000;
      return embedThinkingContent(thinkingContent, mainContent, durationSeconds);
    }

    // Case 2: Check for inline <think> tags in main content
    const parsed = parseStreamingThinkingContent(mainContent);
    if (parsed.thinking) {
      const startTime = inlineStartedAt ?? Date.now();
      const currentEndTime = inlineEndedAt ?? Date.now();
      const durationSeconds = (currentEndTime - startTime) / 1000;
      return embedThinkingContent(parsed.thinking, parsed.content, durationSeconds);
    }

    // Case 3: No thinking content
    return mainContent;
  }

  function buildFinalContent(mainContent: string): string {
    // Case 1: We have reasoning_content
    if (thinkingContent) {
      const endTime = thinkingEndedAt ?? Date.now();
      const durationSeconds = (endTime - (thinkingStartedAt ?? endTime)) / 1000;
      return embedThinkingContent(thinkingContent, mainContent, durationSeconds);
    }

    // Case 2: Inline <think> tags
    if (inlineStartedAt !== null) {
      const parsed = parseStreamingThinkingContent(mainContent);
      if (parsed.thinking) {
        const endTime = inlineEndedAt ?? Date.now();
        const durationSeconds = (endTime - inlineStartedAt) / 1000;
        return embedThinkingContent(parsed.thinking, parsed.content, durationSeconds);
      }
    }

    // Case 3: No thinking
    return mainContent;
  }

  function getState(): ThinkingState {
    return {
      isThinking: thinkingContent.length > 0 && thinkingEndedAt === null,
      thinkingContent,
      startedAt: thinkingStartedAt,
      endedAt: thinkingEndedAt,
    };
  }

  function reset(): void {
    thinkingContent = '';
    thinkingStartedAt = null;
    thinkingEndedAt = null;
    inlineStartedAt = null;
    inlineEndedAt = null;
  }

  return {
    handleReasoningDelta,
    handleContentDelta,
    markMainContentStarted,
    buildDisplayContent,
    buildFinalContent,
    getState,
    reset,
  };
}
