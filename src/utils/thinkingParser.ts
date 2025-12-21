/**
 * Utilities for parsing thinking/reasoning content from messages.
 * 
 * Reasoning models output a "thinking" phase that llama-server extracts
 * to `reasoning_content` field. For persistence and display, thinking
 * content is stored as `<think>...</think>` tags embedded in the message.
 * 
 * Supports multiple tag formats used by different models:
 * - `<think>...</think>` - DeepSeek R1, Qwen3, most reasoning models
 * - `<reasoning>...</reasoning>` - Alternative format
 * - `<seed:think>...</seed:think>` - Seed-OSS models
 * - `<|START_THINKING|>...<|END_THINKING|>` - Command-R7B style
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
 * Normalize different thinking tag formats to standard `<think>` format.
 * This allows consistent handling regardless of which model format is used.
 * 
 * @param text - Text that may contain various thinking tag formats
 * @returns Text with normalized `<think>` tags
 */
export function normalizeThinkingTags(text: string): string {
  if (!text) return text;
  
  let normalized = text;
  
  // Normalize <seed:think> to <think>
  normalized = normalized.replace(/<seed:think>/gi, '<think>');
  normalized = normalized.replace(/<\/seed:think>/gi, '</think>');
  
  // Normalize <|START_THINKING|> to <think>
  normalized = normalized.replace(/<\|START_THINKING\|>/gi, '<think>');
  normalized = normalized.replace(/<\|END_THINKING\|>/gi, '</think>');
  
  // Normalize <reasoning> to <think>
  normalized = normalized.replace(/<reasoning>/gi, '<think>');
  normalized = normalized.replace(/<\/reasoning>/gi, '</think>');
  
  return normalized;
}

/**
 * Parse thinking content from a message that may contain thinking tags.
 * 
 * Supports multiple formats:
 * - `<think>...</think>` - Standard format (DeepSeek R1, Qwen3, most models)
 * - `<reasoning>...</reasoning>` - Alternative format
 * - `<seed:think>...</seed:think>` - Seed-OSS models
 * - `<|START_THINKING|>...<|END_THINKING|>` - Command-R7B style
 * - `<think duration="X.X">...</think>` - With duration metadata (normalized output)
 * 
 * @param text - The full message text that may contain thinking tags
 * @returns Parsed thinking content and main content
 */
export function parseThinkingContent(text: string): ParsedThinkingContent {
  if (!text) {
    return { thinking: null, content: '', durationSeconds: null };
  }

  // Normalize tags first for consistent parsing
  const normalized = normalizeThinkingTags(text);

  // Match <think> tags with optional duration attribute
  // Supports: <think>...</think> or <think duration="5.2">...</think>
  const thinkingRegex = /^<think(?:\s+duration="([\d.]+)")?\s*>([\s\S]*?)<\/think>\s*/;
  const match = normalized.match(thinkingRegex);

  if (!match) {
    return { thinking: null, content: text, durationSeconds: null };
  }

  const durationStr = match[1];
  const thinkingContent = match[2]?.trim() || null;
  const remainingContent = normalized.slice(match[0].length);
  const durationSeconds = durationStr ? parseFloat(durationStr) : null;

  return {
    thinking: thinkingContent,
    content: remainingContent,
    durationSeconds,
  };
}

/**
 * Embed thinking content into a message using `<think>` tags.
 * Always uses standard `<think>` format for consistency.
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
 * Checks for all known thinking tag formats.
 * 
 * @param text - Text to check
 * @returns True if the text starts with a thinking tag
 */
export function hasThinkingContent(text: string): boolean {
  const trimmed = text.trimStart().toLowerCase();
  return (
    trimmed.startsWith('<think') ||
    trimmed.startsWith('<reasoning') ||
    trimmed.startsWith('<seed:think') ||
    trimmed.startsWith('<|start_thinking|')
  );
}

/**
 * Extract thinking content from streaming text that may have incomplete tags.
 * Used during streaming when we receive content with inline thinking tags
 * (e.g., when server uses `--reasoning-format none`).
 * 
 * Supports all known thinking tag formats by normalizing first.
 * 
 * @param text - Streaming text that may contain partial thinking tags
 * @returns Object with extracted thinking, remaining content, and whether thinking is complete
 */
export function parseStreamingThinkingContent(text: string): {
  thinking: string;
  content: string;
  isThinkingComplete: boolean;
} {
  // Normalize tags for consistent parsing
  const normalized = normalizeThinkingTags(text);
  const trimmed = normalized.trimStart();
  
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
