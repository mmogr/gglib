/**
 * Utilities for wrapping thinking/reasoning content in <think> tags.
 * 
 * @module wrapThink
 */

/**
 * Wrap text in <think> tags for thinking/reasoning content.
 * 
 * - Trims input whitespace
 * - Returns empty string for whitespace-only input (prevents empty blocks)
 * - Returns '<think>\n{trimmed}\n</think>'
 * - Optionally adds `duration` attribute if provided
 * 
 * @param text - The thinking/reasoning text to wrap
 * @param durationSeconds - Optional duration in seconds to include in the tag
 * @returns Wrapped text or empty string if input is empty/whitespace
 */
export function wrapThink(text: string, durationSeconds?: number | null): string {
  const trimmed = text.trim();
  if (!trimmed) {
    return '';
  }
  
  // Build duration attribute if provided and valid
  const durationAttr = 
    (durationSeconds != null && Number.isFinite(durationSeconds))
      ? ` duration="${durationSeconds.toFixed(1)}"`
      : '';
  
  return `<think${durationAttr}>\n${trimmed}\n</think>`;
}

/**
 * Check if text is already wrapped in <think> tags.
 * 
 * Ignores leading whitespace to avoid false negatives.
 * 
 * @param text - Text to check
 * @returns True if text starts with a <think tag (after trimming leading whitespace)
 */
export function isWrappedThink(text: string): boolean {
  return text.trimStart().startsWith('<think');
}
