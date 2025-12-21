/**
 * Message sanitization for llama-server requests.
 * 
 * Strips <think> tags and removes unsupported fields before sending
 * messages to llama-server, preventing Jinja template errors.
 */

const THINK_TAG_REGEX = /<think[^>]*>[\s\S]*?<\/think>/gi;

/**
 * Strip all <think>...</think> tags from content.
 * 
 * Handles tags with attributes and multiline content.
 */
export function stripThinkTags(content: string): string {
  return content.replace(THINK_TAG_REGEX, '').trim();
}

/**
 * Sanitize messages for llama-server API calls.
 * 
 * - Strips <think> tags from content
 * - Returns only { role, content } (no reasoning_content, etc.)
 * - Works with messages from DB (old data) or runtime (new data)
 * 
 * @param messages - Messages with any shape (as long as they have role + content)
 * @returns Clean messages safe for llama-server
 */
export function sanitizeMessagesForLlamaServer<T extends { role: string; content: string }>(
  messages: T[],
): Array<{ role: string; content: string }> {
  return messages.map(({ role, content }) => ({
    role,
    content: stripThinkTags(content),
  }));
}
