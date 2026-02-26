/**
 * Utilities for sanitizing and displaying MCP tool names.
 *
 * MCP tool names can contain arbitrary characters (dots, spaces, hyphens, unicode)
 * that are invalid for OpenAI function calling, which requires `^[a-zA-Z0-9_-]{1,64}$`.
 * These pure, dependency-free functions handle sanitization, collision detection,
 * and display formatting.
 */

// =============================================================================
// Constants
// =============================================================================

/** Maximum length enforced by the OpenAI function-calling API. */
const MAX_TOOL_NAME_LENGTH = 64;

/** Fallback name used when sanitization produces an empty string. */
const FALLBACK_NAME = 'unnamed_tool';

// =============================================================================
// sanitizeToolName
// =============================================================================

/**
 * Sanitizes a raw tool name string for OpenAI function-calling compatibility.
 *
 * Transforms the input so the result always matches `^[a-zA-Z0-9_-]{1,64}$`:
 * - Invalid characters (anything not `a-zA-Z0-9_-`) are replaced with `_`
 * - Consecutive underscores are collapsed into a single `_`
 * - Leading and trailing underscores are trimmed
 * - Result is truncated to 64 characters
 * - Empty result (e.g. all-special input) falls back to `'unnamed_tool'`
 *
 * Hyphens are intentionally preserved — they are valid per the OpenAI spec
 * and commonly used in MCP tool names (e.g. `get-weather`).
 *
 * Pass the fully-namespaced string (including the `mcp_${serverId}_` prefix) so
 * the entire result is validated within the 64-character budget.
 *
 * @example
 * sanitizeToolName('mcp_my.server_get data!') // → 'mcp_my_server_get_data'
 * sanitizeToolName('get-weather')             // → 'get-weather'
 * sanitizeToolName('file.read')               // → 'file_read'
 * sanitizeToolName('!!!')                     // → 'unnamed_tool'
 */
export function sanitizeToolName(raw: string): string {
  return (
    raw
      .replace(/[^a-zA-Z0-9_-]/g, '_')  // Replace invalid chars with underscore
      .replace(/_+/g, '_')              // Collapse consecutive underscores
      .replace(/^_+|_+$/g, '')          // Trim leading/trailing underscores
      .slice(0, MAX_TOOL_NAME_LENGTH)   // Enforce max length
    || FALLBACK_NAME                    // Fallback when result is empty
  );
}

// =============================================================================
// detectCollisions
// =============================================================================

/**
 * Finds tool name collisions — cases where two or more distinct raw names
 * sanitize to the same output string.
 *
 * Pass fully-namespaced names (e.g. `mcp_${serverId}_${tool.name}`) so that
 * 64-character truncation collisions are caught before any registration occurs.
 *
 * @param names - Fully-namespaced tool name strings to check
 * @returns Map of `sanitizedName → originalNames[]` containing only entries
 *          with two or more originals (i.e. actual collisions)
 *
 * @example
 * detectCollisions(['mcp_s_ab!', 'mcp_s_ab?'])
 * // → Map { 'mcp_s_ab' → ['mcp_s_ab!', 'mcp_s_ab?'] }
 */
export function detectCollisions(names: string[]): Map<string, string[]> {
  const grouped = new Map<string, string[]>();

  for (const name of names) {
    const sanitized = sanitizeToolName(name);
    const existing = grouped.get(sanitized) ?? [];
    existing.push(name);
    grouped.set(sanitized, existing);
  }

  // Return only entries where two or more originals collide
  return new Map(
    [...grouped.entries()].filter(([, originals]) => originals.length > 1),
  );
}

// =============================================================================
// formatToolDisplayName
// =============================================================================

/**
 * Converts a raw MCP tool name into a human-readable Title Case string.
 *
 * Splits on hyphens, underscores, dots, and whitespace so that any of the
 * common MCP naming conventions are handled correctly.
 *
 * @example
 * formatToolDisplayName('get-weather')      // → 'Get Weather'
 * formatToolDisplayName('file.read')        // → 'File Read'
 * formatToolDisplayName('my tool')          // → 'My Tool'
 * formatToolDisplayName('get_current_time') // → 'Get Current Time'
 */
export function formatToolDisplayName(raw: string): string {
  return raw
    .split(/[-_.\s]+/)
    .filter((word) => word.length > 0)
    .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
    .join(' ');
}
