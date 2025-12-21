/**
 * Parse and clean title generation model responses.
 * 
 * Handles various model output formats robustly:
 * - "Title: Something" → "Something"
 * - '"Something"' → "Something"
 * - 'Here is a title: "Something"' → "Something"
 * - JSON: {"title": "Something"} → "Something"
 */

import { stripThinkTags } from './sanitizeMessages';

/**
 * Parse generated title from model response.
 * 
 * This is intentionally tolerant of various model output formats
 * to avoid brittle parsing errors. The model might:
 * - Add "Title:" prefix
 * - Wrap in quotes or backticks
 * - Return JSON with a title field
 * - Add explanatory text before the actual title
 * 
 * @param raw - Raw model response content
 * @returns Cleaned title string
 * @throws Error if no valid title can be extracted
 */
export function parseGeneratedTitle(raw: string): string {
  // 1. Remove any stray <think> tags that might slip through
  const noThink = stripThinkTags(raw).trim();

  if (!noThink) {
    throw new Error('Model returned empty content for title generation.');
  }

  // 2. Try parsing as JSON first (some models return structured responses)
  try {
    const json = JSON.parse(noThink);
    if (json && typeof json.title === 'string' && json.title.trim()) {
      return cleanTitle(json.title);
    }
  } catch {
    // Not JSON, that's fine - continue with text parsing
  }

  // 3. Split into lines, find the first non-empty one
  // This handles models that add explanatory text before the title
  const lines = noThink.split('\n').map((l) => l.trim());
  const firstLine = lines.find((l) => l.length > 0);

  if (!firstLine) {
    throw new Error('Model returned only whitespace for title generation.');
  }

  // 4. Remove common prefixes like "Title:", "Here's a title:", etc.
  let title = firstLine.replace(/^(?:title|here(?:'?s)?\s+(?:a\s+)?(?:title|suggestion)):\s*/gi, '');

  // 5. Clean up the extracted title
  return cleanTitle(title);
}

/**
 * Clean extracted title text.
 * 
 * - Strips surrounding quotes, backticks, markdown
 * - Removes trailing periods
 * - Limits length
 * - Falls back to "New Chat" if empty after cleaning
 */
function cleanTitle(title: string): string {
  let cleaned = title.trim();

  // Remove surrounding quotes, backticks, or smart quotes
  cleaned = cleaned.replace(/^["'`""'']+/, '').replace(/["'`""'']+$/, '');

  // Remove trailing periods (common in sentences)
  cleaned = cleaned.replace(/\.+$/, '');

  // Remove markdown bold/italic
  cleaned = cleaned.replace(/^\*\*?/, '').replace(/\*\*?$/, '');
  cleaned = cleaned.replace(/^__?/, '').replace(/__?$/, '');

  // Final trim and length limit
  cleaned = cleaned.trim().slice(0, 100);

  if (!cleaned) {
    return 'New Chat';
  }

  return cleaned;
}
