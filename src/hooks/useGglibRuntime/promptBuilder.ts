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

// =============================================================================
// Message-array injection
// =============================================================================

/**
 * Inject a set of `PromptLayer` fragments into an OpenAI-style message array,
 * composing them into the first system message via `buildSystemPrompt`.
 *
 * ### Immutability contract
 * - **Always returns a new array** (via `slice()`).
 * - The system-message object is replaced with a spread copy:
 *   `cloned[idx] = { ...original, content: composed }`.
 *   **Never** `cloned[idx].content = composed` — `slice()` is a shallow copy,
 *   so direct property mutation would bleed back into the caller's array.
 *
 * ### Double-injection safety
 * `buildSystemPrompt` is **not** idempotent — it does not check whether a
 * layer is already present before appending.  Sequential calls (e.g. once in
 * `runAgenticLoop.ts` for working memory, then again in
 * `streamModelResponse.ts` for tool/format layers) are safe **only** because
 * the caller keeps its `apiMessages` array pristine and passes a fresh clone
 * into each call.  The architectural guarantee lives in the caller, not here.
 *
 * @param messages - Source message array; never mutated.
 * @param layers   - Layers to inject.  If empty, returns a defensive clone.
 * @returns New array with the composed system message in place.
 */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export function injectPromptLayers(messages: readonly any[], layers: PromptLayer[]): any[] {
  // Defensive clone even when there is nothing to inject.
  if (layers.length === 0) return messages.slice();

  const cloned = messages.slice();

  const sysIdx = cloned.findIndex(
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (m: any) => m?.role === 'system' && typeof m.content === 'string',
  );

  if (sysIdx >= 0) {
    // ⚠ Spread-replace the object — do NOT assign .content directly.
    cloned[sysIdx] = {
      ...cloned[sysIdx],
      content: buildSystemPrompt(cloned[sysIdx].content as string, layers),
    };
    return cloned;
  }

  // No existing system message — prepend one built from the layers alone.
  return [{ role: 'system', content: buildSystemPrompt('', layers) }, ...messages];
}

// =============================================================================
// Working-memory layer factory
// =============================================================================

/**
 * Create a transient `PromptLayer` from pre-formatted working-memory digest
 * lines.  Priority 300 places it after `TOOL_INSTRUCTIONS_LAYER` (100) and
 * `FORMAT_REMINDER_LAYER` (200) so dynamic, iteration-specific context lands
 * last in the composed system message — where models tend to weight it most.
 *
 * @param digestLines - One pre-formatted line per tool digest, e.g.
 *   `"- search (ok): <summary>"`.
 */
export function createWorkingMemoryLayer(digestLines: string[]): PromptLayer {
  return {
    id: 'working-memory',
    content: '## Working Memory\n' + digestLines.join('\n'),
    position: 'append',
    priority: 300,
  };
}
