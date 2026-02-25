/**
 * Prompt composition utilities — additive injection of prompt layers.
 *
 * Instead of hot-swapping system prompts by exact string match, callers
 * build a final system prompt by composing a base string with an ordered
 * set of `PromptLayer` fragments. Layers are sorted by `priority` (lower
 * value = earlier), then split into prepends (inserted before the base)
 * and appends (inserted after the base). Empty segments are filtered
 * before joining so no dangling newlines appear when the base is empty.
 *
 * @module promptBuilder
 */

// =============================================================================
// Types
// =============================================================================

export interface PromptLayer {
  /** Unique identifier for this layer. */
  id: string;
  /** Prompt fragment text to inject. */
  content: string;
  /** Whether this layer is inserted before or after the base prompt. */
  position: 'prepend' | 'append';
  /** Ordering key — lower numbers appear first within each position group. */
  priority: number;
}

// =============================================================================
// Core builder
// =============================================================================

/**
 * Compose a final system prompt from a base string and an ordered list of
 * `PromptLayer` fragments.
 *
 * Layers are sorted by `priority`, then split into `prepend` and `append`
 * groups. The final string is:
 *
 *   [...prepends, basePrompt, ...appends].join('\n\n')
 *
 * Empty or whitespace-only segments are filtered out before joining so that
 * an empty `basePrompt` never introduces a leading or trailing blank block.
 *
 * @param basePrompt - The user-supplied or default system prompt.
 * @param layers     - Layers to inject around the base prompt.
 * @returns Composed system prompt string.
 */
export function buildSystemPrompt(basePrompt: string, layers: PromptLayer[]): string {
  const sorted = [...layers].sort((a, b) => a.priority - b.priority);
  const prepends = sorted.filter(l => l.position === 'prepend').map(l => l.content);
  const appends  = sorted.filter(l => l.position === 'append').map(l => l.content);

  return [...prepends, basePrompt, ...appends]
    .filter(s => s && s.trim() !== '')
    .join('\n\n');
}

// =============================================================================
// Standard layers
// =============================================================================

/**
 * Tool-usage instructions appended when the model has access to tools.
 * This is the single source of truth for tool guidance text; it is injected
 * additively on top of whichever base system prompt the user has configured.
 */
export const TOOL_INSTRUCTIONS_LAYER: PromptLayer = {
  id: 'tool-instructions',
  content:
    'When you need information or actions, use the available tools rather than guessing.\n' +
    'Keep working explanations concise. When you have enough information, provide your final answer directly.',
  position: 'append',
  priority: 100,
};

/**
 * General format reminder appended after all tool instructions.
 * Intended as a lightweight nudge; content can be refined in a later phase.
 */
export const FORMAT_REMINDER = 'Please respond clearly and directly.';

export const FORMAT_REMINDER_LAYER: PromptLayer = {
  id: 'format-reminder',
  content: FORMAT_REMINDER,
  position: 'append',
  priority: 200,
};
