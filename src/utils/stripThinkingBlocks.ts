/**
 * Strip internal chain-of-thought / thinking blocks from LLM text.
 *
 * Some models inline thinking content within the visible response text
 * using various tag conventions. This utility removes all recognised
 * variants so the remaining text can be spoken via TTS without leaking
 * reasoning internals.
 *
 * @module utils/stripThinkingBlocks
 */

/**
 * Remove `<think>`, `<reasoning>`, `<seed:think>`, and
 * `<|START_THINKING|>…<|END_THINKING|>` blocks from text.
 */
export function stripThinkingBlocks(text: string): string {
  return text
    .replace(/<think[^>]*>[\s\S]*?<\/think>/gi, '')
    .replace(/<reasoning>[\s\S]*?<\/reasoning>/gi, '')
    .replace(/<seed:think>[\s\S]*?<\/seed:think>/gi, '')
    .replace(/<\|START_THINKING\|>[\s\S]*?<\|END_THINKING\|>/gi, '')
    .trim();
}
