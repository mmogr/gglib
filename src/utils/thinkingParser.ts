/**
 * Utilities for parsing thinking/reasoning content from messages.
 * 
 * Reasoning models output a "thinking" phase that llama-server extracts
 * to `reasoning_content` field. For persistence and display, thinking
 * content is stored as `<think>...</think>` tags embedded in the message.
 */

export interface ParsedThinkingContent {
  /** The thinking/reasoning content, or null if none */
  thinking: string | null;
  /** The main content with thinking tags removed */
  content: string;
  /** Duration in seconds if metadata was present, or null */
  durationSeconds: number | null;
}

/**
 * Parse thinking content from a message that may contain `<think>...</think>` tags.
 * 
 * Supports formats:
 * - `<think>...</think>` - Standard format used by most models
 * - `<think duration="X.X">...</think>` - With duration metadata
 * 
 * @param text - The full message text that may contain thinking tags
 * @returns Parsed thinking content and main content
 */
export function parseThinkingContent(text: string): ParsedThinkingContent {
  if (!text) {
    return { thinking: null, content: '', durationSeconds: null };
  }

  // Match <think> tags with optional duration attribute
  // Supports: <think>...</think> or <think duration="5.2">...</think>
  const thinkingRegex = /^<think(?:\s+duration="([\d.]+)")?\s*>([\s\S]*?)<\/think>\s*/;
  const match = text.match(thinkingRegex);

  if (!match) {
    return { thinking: null, content: text, durationSeconds: null };
  }

  const durationStr = match[1];
  const thinkingContent = match[2]?.trim() || null;
  const remainingContent = text.slice(match[0].length);
  const durationSeconds = durationStr ? parseFloat(durationStr) : null;

  return {
    thinking: thinkingContent,
    content: remainingContent,
    durationSeconds,
  };
}

/**
 * Embed thinking content into a message using `<think>` tags.
 * 
 * @param thinking - The thinking content to embed
 * @param content - The main content
 * @param durationSeconds - Optional duration in seconds to include as metadata
 * @returns Combined message with thinking embedded as tags
 */
export function embedThinkingContent(
  thinking: string | null,
  content: string,
  durationSeconds?: number | null
): string {
  if (!thinking) {
    return content;
  }

  const durationAttr = durationSeconds != null 
    ? ` duration="${durationSeconds.toFixed(1)}"` 
    : '';
  
  return `<think${durationAttr}>${thinking}</think>\n${content}`;
}

/**
 * Check if content appears to contain thinking tags (for streaming detection).
 * This is a lightweight check for UI purposes.
 * 
 * @param text - Text to check
 * @returns True if the text starts with a thinking tag
 */
export function hasThinkingContent(text: string): boolean {
  return text.trimStart().startsWith('<think');
}

/**
 * Extract thinking content from streaming text that may have incomplete tags.
 * Used during streaming when we receive content with inline `<think>` tags
 * (e.g., when server uses `--reasoning-format none`).
 * 
 * @param text - Streaming text that may contain partial thinking tags
 * @returns Object with extracted thinking, remaining content, and whether thinking is complete
 */
export function parseStreamingThinkingContent(text: string): {
  thinking: string;
  content: string;
  isThinkingComplete: boolean;
} {
  const trimmed = text.trimStart();
  
  // Check if we have a complete thinking block
  const completeMatch = trimmed.match(/^<think(?:\s+[^>]*)?>([\s\S]*?)<\/think>\s*([\s\S]*)$/);
  if (completeMatch) {
    return {
      thinking: completeMatch[1]?.trim() || '',
      content: completeMatch[2] || '',
      isThinkingComplete: true,
    };
  }

  // Check if we have a partial thinking block (started but not ended)
  const partialMatch = trimmed.match(/^<think(?:\s+[^>]*)?>([\s\S]*)$/);
  if (partialMatch) {
    return {
      thinking: partialMatch[1] || '',
      content: '',
      isThinkingComplete: false,
    };
  }

  // No thinking tags
  return {
    thinking: '',
    content: text,
    isThinkingComplete: true,
  };
}

/**
 * Format duration for display.
 * 
 * @param seconds - Duration in seconds
 * @returns Formatted string like "5.2s" or "1m 23s"
 */
export function formatThinkingDuration(seconds: number): string {
  if (seconds < 60) {
    return `${seconds.toFixed(1)}s`;
  }
  const minutes = Math.floor(seconds / 60);
  const remainingSeconds = seconds % 60;
  return `${minutes}m ${remainingSeconds.toFixed(0)}s`;
}
